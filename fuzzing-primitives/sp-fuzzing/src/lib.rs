//! A lightweight [`Externalities`] implementation for fuzzing.
//!
//! [`FuzzingExternalities`] replaces [`sp_state_machine::BasicExternalities`] by dropping
//! all cryptographic hashing and Merkle-trie operations. Committed storage lives in a
//! plain `BTreeMap`; in-flight transactional writes live in a stack of `HashMap` overlay
//! layers; `storage_root` / `storage_hash` return fixed dummy values.
//!
//! The overlay stack replaces full `BTreeMap::clone()` snapshots on every
//! `storage_start_transaction`, making each layer cost O(writes-in-layer) rather than
//! O(total-state). Every dispatch wraps itself in a transactional layer, and nested
//! batch / proxy / multisig calls stack more on top — so this is the dominant
//! performance lever for fuzzing throughput.

use codec::{Compact, CompactLen, Decode, Encode};
use core::any::{Any, TypeId};
use sp_core::storage::{
    well_known_keys::is_child_storage_key, ChildInfo, StateVersion, Storage, StorageChild,
    TrackedStorageKey,
};
use sp_externalities::{Extension, Extensions, MultiRemovalResults};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Fixed 32-byte dummy hash returned by `storage_root`, `storage_hash`, etc.
const DUMMY_HASH: [u8; 32] = [0u8; 32];

type StorageKey = Vec<u8>;
type StorageValue = Vec<u8>;

// ---------------------------------------------------------------------------
// Transactional overlay
// ---------------------------------------------------------------------------

/// One layer of pending changes. `Some(v)` is a write; `None` is an explicit delete.
#[derive(Default)]
struct Layer {
    top: HashMap<StorageKey, Option<StorageValue>>,
    children: HashMap<StorageKey, ChildLayer>,
}

struct ChildLayer {
    info: ChildInfo,
    /// `true` once `kill_child_storage` was called within this layer. Hides all state
    /// for this child below this layer; only `writes` recorded afterwards contribute.
    killed: bool,
    writes: HashMap<StorageKey, Option<StorageValue>>,
}

impl ChildLayer {
    fn new(info: ChildInfo) -> Self {
        Self { info, killed: false, writes: HashMap::new() }
    }
}

// ---------------------------------------------------------------------------
// FuzzingExternalities
// ---------------------------------------------------------------------------

/// Externalities backed by a committed base map plus an overlay stack.
pub struct FuzzingExternalities {
    base_top: BTreeMap<StorageKey, StorageValue>,
    base_children: BTreeMap<StorageKey, (ChildInfo, BTreeMap<StorageKey, StorageValue>)>,
    layers: Vec<Layer>,
    extensions: Extensions,
}

impl Default for FuzzingExternalities {
    fn default() -> Self {
        Self::new(Storage::default())
    }
}

impl FuzzingExternalities {
    /// Create from a [`Storage`] genesis snapshot.
    pub fn new(storage: Storage) -> Self {
        let base_children = storage
            .children_default
            .into_iter()
            .map(|(k, child)| (k, (child.child_info, child.data)))
            .collect();
        Self {
            base_top: storage.top,
            base_children,
            layers: Vec::new(),
            extensions: Extensions::default(),
        }
    }

    /// Convert back into a [`Storage`]. Any still-open transaction layers are flushed
    /// bottom-up, as if they had been committed.
    pub fn into_storage(self) -> Storage {
        let Self { mut base_top, mut base_children, layers, .. } = self;
        for layer in layers {
            merge_top_into_base(&mut base_top, layer.top);
            merge_children_into_base(&mut base_children, layer.children);
        }
        Storage {
            top: base_top,
            children_default: base_children
                .into_iter()
                .map(|(k, (info, data))| (k, StorageChild { data, child_info: info }))
                .collect(),
        }
    }

    /// Run `f` with `self` installed as the thread-local externalities.
    pub fn execute_with<R>(&mut self, f: impl FnOnce() -> R) -> R {
        sp_externalities::set_and_run_with_externalities(self, f)
    }

    /// Convenience: take ownership of `storage`, run `f`, then write back.
    pub fn execute_with_storage<R>(storage: &mut Storage, f: impl FnOnce() -> R) -> R {
        let mut ext = Self::new(core::mem::take(storage));
        let r = ext.execute_with(f);
        *storage = ext.into_storage();
        r
    }

    /// Register an extension.
    pub fn register_extension(&mut self, ext: impl Extension) {
        self.extensions.register(ext);
    }

    // -----------------------------------------------------------------------
    // Internal resolvers
    // -----------------------------------------------------------------------

    /// Return the currently visible top-level value for `key`, walking layers top-down.
    fn resolve_top(&self, key: &[u8]) -> Option<&StorageValue> {
        for layer in self.layers.iter().rev() {
            if let Some(v) = layer.top.get(key) {
                return v.as_ref();
            }
        }
        self.base_top.get(key)
    }

    /// Same, for child storage; respects `killed`.
    fn resolve_child(&self, storage_key: &[u8], key: &[u8]) -> Option<&StorageValue> {
        for layer in self.layers.iter().rev() {
            if let Some(child) = layer.children.get(storage_key) {
                if let Some(v) = child.writes.get(key) {
                    return v.as_ref();
                }
                if child.killed {
                    return None;
                }
            }
        }
        self.base_children.get(storage_key).and_then(|(_, m)| m.get(key))
    }

    /// Write a top-level key into the current layer, or into the base if none.
    fn set_top(&mut self, key: StorageKey, value: Option<StorageValue>) {
        if let Some(layer) = self.layers.last_mut() {
            layer.top.insert(key, value);
        } else {
            match value {
                Some(v) => {
                    self.base_top.insert(key, v);
                }
                None => {
                    self.base_top.remove(&key);
                }
            }
        }
    }

    /// Write a child key into the current layer, or into the base if none.
    fn set_child(
        &mut self,
        child_info: &ChildInfo,
        key: StorageKey,
        value: Option<StorageValue>,
    ) {
        if let Some(layer) = self.layers.last_mut() {
            layer
                .children
                .entry(child_info.storage_key().to_vec())
                .or_insert_with(|| ChildLayer::new(child_info.clone()))
                .writes
                .insert(key, value);
        } else {
            let entry = self
                .base_children
                .entry(child_info.storage_key().to_vec())
                .or_insert_with(|| (child_info.clone(), BTreeMap::new()));
            match value {
                Some(v) => {
                    entry.1.insert(key, v);
                }
                None => {
                    entry.1.remove(&key);
                }
            }
        }
    }

    /// Highest layer index where this child was killed, if any. Base and layers below
    /// this index contribute nothing to reads/iteration for this child.
    fn child_kill_floor(&self, storage_key: &[u8]) -> Option<usize> {
        for (i, layer) in self.layers.iter().enumerate().rev() {
            if let Some(child) = layer.children.get(storage_key) {
                if child.killed {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Currently visible top-level keys sharing `prefix`, sorted.
    fn visible_top_keys_with_prefix(&self, prefix: &[u8]) -> Vec<StorageKey> {
        use std::ops::Bound;
        let mut candidates: BTreeSet<StorageKey> = BTreeSet::new();
        for (k, _) in self
            .base_top
            .range::<[u8], _>((Bound::Included(prefix), Bound::Unbounded))
        {
            if !k.starts_with(prefix) {
                break;
            }
            candidates.insert(k.clone());
        }
        for layer in &self.layers {
            for k in layer.top.keys() {
                if k.starts_with(prefix) {
                    candidates.insert(k.clone());
                }
            }
        }
        candidates
            .into_iter()
            .filter(|k| self.resolve_top(k).is_some())
            .collect()
    }

    /// Same, for a child storage.
    fn visible_child_keys_with_prefix(
        &self,
        storage_key: &[u8],
        prefix: &[u8],
    ) -> Vec<StorageKey> {
        use std::ops::Bound;
        let kill_floor = self.child_kill_floor(storage_key);
        let mut candidates: BTreeSet<StorageKey> = BTreeSet::new();
        if kill_floor.is_none() {
            if let Some((_, base)) = self.base_children.get(storage_key) {
                for (k, _) in base.range::<[u8], _>((Bound::Included(prefix), Bound::Unbounded)) {
                    if !k.starts_with(prefix) {
                        break;
                    }
                    candidates.insert(k.clone());
                }
            }
        }
        let start = kill_floor.unwrap_or(0);
        for layer in &self.layers[start..] {
            if let Some(child) = layer.children.get(storage_key) {
                for k in child.writes.keys() {
                    if k.starts_with(prefix) {
                        candidates.insert(k.clone());
                    }
                }
            }
        }
        candidates
            .into_iter()
            .filter(|k| self.resolve_child(storage_key, k).is_some())
            .collect()
    }

    /// Approximate count of currently visible keys in a child storage.
    fn visible_child_key_count(&self, storage_key: &[u8]) -> u32 {
        let kill_floor = self.child_kill_floor(storage_key);
        let mut visible: BTreeMap<StorageKey, bool> = BTreeMap::new();
        if kill_floor.is_none() {
            if let Some((_, base)) = self.base_children.get(storage_key) {
                for k in base.keys() {
                    visible.insert(k.clone(), true);
                }
            }
        }
        let start = kill_floor.unwrap_or(0);
        for layer in &self.layers[start..] {
            if let Some(child) = layer.children.get(storage_key) {
                for (k, v) in &child.writes {
                    visible.insert(k.clone(), v.is_some());
                }
            }
        }
        visible.values().filter(|&&exists| exists).count() as u32
    }
}

// ---------------------------------------------------------------------------
// Externalities impl
// ---------------------------------------------------------------------------

impl sp_core::traits::Externalities for FuzzingExternalities {
    fn set_offchain_storage(&mut self, _key: &[u8], _value: Option<&[u8]>) {}

    fn storage(&mut self, key: &[u8]) -> Option<StorageValue> {
        self.resolve_top(key).cloned()
    }

    fn storage_hash(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        self.resolve_top(key).map(|_| DUMMY_HASH.to_vec())
    }

    fn child_storage(&mut self, child_info: &ChildInfo, key: &[u8]) -> Option<StorageValue> {
        self.resolve_child(child_info.storage_key(), key).cloned()
    }

    fn child_storage_hash(&mut self, child_info: &ChildInfo, key: &[u8]) -> Option<Vec<u8>> {
        self.resolve_child(child_info.storage_key(), key)
            .map(|_| DUMMY_HASH.to_vec())
    }

    fn next_storage_key(&mut self, key: &[u8]) -> Option<StorageKey> {
        use std::ops::Bound;
        // Fast path: no layers, just a range query on base.
        if self.layers.is_empty() {
            return self
                .base_top
                .range::<[u8], _>((Bound::Excluded(key), Bound::Unbounded))
                .next()
                .map(|(k, _)| k.clone());
        }
        // Stream-merge the sorted base iterator with the overlay candidates.
        let mut overlay: Vec<&[u8]> = Vec::new();
        for layer in &self.layers {
            for k in layer.top.keys() {
                if k.as_slice() > key {
                    overlay.push(k.as_slice());
                }
            }
        }
        overlay.sort_unstable();
        overlay.dedup();
        let mut overlay_iter = overlay.into_iter().peekable();
        let mut base_iter = self
            .base_top
            .range::<[u8], _>((Bound::Excluded(key), Bound::Unbounded))
            .map(|(k, _)| k.as_slice())
            .peekable();
        loop {
            let candidate: &[u8] = match (base_iter.peek().copied(), overlay_iter.peek().copied())
            {
                (None, None) => return None,
                (Some(b), None) => {
                    base_iter.next();
                    b
                }
                (None, Some(o)) => {
                    overlay_iter.next();
                    o
                }
                (Some(b), Some(o)) if b < o => {
                    base_iter.next();
                    b
                }
                (Some(b), Some(o)) if b > o => {
                    overlay_iter.next();
                    o
                }
                (Some(b), Some(_)) => {
                    base_iter.next();
                    overlay_iter.next();
                    b
                }
            };
            if self.resolve_top(candidate).is_some() {
                return Some(candidate.to_vec());
            }
        }
    }

    fn next_child_storage_key(
        &mut self,
        child_info: &ChildInfo,
        key: &[u8],
    ) -> Option<StorageKey> {
        use std::ops::Bound;
        let storage_key = child_info.storage_key();
        let kill_floor = self.child_kill_floor(storage_key);
        let mut candidates: BTreeSet<StorageKey> = BTreeSet::new();
        if kill_floor.is_none() {
            if let Some((_, base)) = self.base_children.get(storage_key) {
                for (k, _) in base.range::<[u8], _>((Bound::Excluded(key), Bound::Unbounded)) {
                    candidates.insert(k.clone());
                }
            }
        }
        let start = kill_floor.unwrap_or(0);
        for layer in &self.layers[start..] {
            if let Some(child) = layer.children.get(storage_key) {
                for k in child.writes.keys() {
                    if k.as_slice() > key {
                        candidates.insert(k.clone());
                    }
                }
            }
        }
        candidates
            .into_iter()
            .find(|k| self.resolve_child(storage_key, k).is_some())
    }

    fn place_storage(&mut self, key: StorageKey, value: Option<StorageValue>) {
        if is_child_storage_key(&key) {
            return;
        }
        self.set_top(key, value);
    }

    fn place_child_storage(
        &mut self,
        child_info: &ChildInfo,
        key: StorageKey,
        value: Option<StorageValue>,
    ) {
        self.set_child(child_info, key, value);
    }

    fn kill_child_storage(
        &mut self,
        child_info: &ChildInfo,
        _maybe_limit: Option<u32>,
        _maybe_cursor: Option<&[u8]>,
    ) -> MultiRemovalResults {
        let storage_key = child_info.storage_key();
        let count = self.visible_child_key_count(storage_key);
        if let Some(layer) = self.layers.last_mut() {
            let entry = layer
                .children
                .entry(storage_key.to_vec())
                .or_insert_with(|| ChildLayer::new(child_info.clone()));
            entry.killed = true;
            entry.writes.clear();
        } else {
            self.base_children.remove(storage_key);
        }
        MultiRemovalResults { maybe_cursor: None, backend: count, unique: count, loops: count }
    }

    fn clear_prefix(
        &mut self,
        prefix: &[u8],
        _maybe_limit: Option<u32>,
        _maybe_cursor: Option<&[u8]>,
    ) -> MultiRemovalResults {
        if is_child_storage_key(prefix) {
            return MultiRemovalResults {
                maybe_cursor: Some(prefix.to_vec()),
                backend: 0,
                unique: 0,
                loops: 0,
            };
        }
        let keys = self.visible_top_keys_with_prefix(prefix);
        let count = keys.len() as u32;
        for k in keys {
            self.set_top(k, None);
        }
        MultiRemovalResults { maybe_cursor: None, backend: count, unique: count, loops: count }
    }

    fn clear_child_prefix(
        &mut self,
        child_info: &ChildInfo,
        prefix: &[u8],
        _maybe_limit: Option<u32>,
        _maybe_cursor: Option<&[u8]>,
    ) -> MultiRemovalResults {
        let storage_key = child_info.storage_key();
        let keys = self.visible_child_keys_with_prefix(storage_key, prefix);
        let count = keys.len() as u32;
        for k in keys {
            self.set_child(child_info, k, None);
        }
        MultiRemovalResults { maybe_cursor: None, backend: count, unique: count, loops: count }
    }

    fn storage_append(&mut self, key: StorageKey, element: StorageValue) {
        // Fast path: if the key is already materialised in the top layer as a value,
        // append in place. This keeps repeated appends (e.g. System::Events) O(1) per
        // call after the first one in a given layer.
        if let Some(slot) = self.layers.last_mut().and_then(|l| l.top.get_mut(&key)) {
            if let Some(v) = slot.as_mut() {
                append_to_storage_vec(v, &element);
                return;
            }
        }
        // Otherwise: resolve currently visible value, append, write back to the right
        // destination (top layer if any, else base).
        let mut v = self.resolve_top(&key).cloned().unwrap_or_default();
        append_to_storage_vec(&mut v, &element);
        self.set_top(key, Some(v));
    }

    fn storage_root(&mut self, _state_version: StateVersion) -> Vec<u8> {
        DUMMY_HASH.to_vec()
    }

    fn child_storage_root(
        &mut self,
        _child_info: &ChildInfo,
        _state_version: StateVersion,
    ) -> Vec<u8> {
        DUMMY_HASH.to_vec()
    }

    fn storage_start_transaction(&mut self) {
        self.layers.push(Layer::default());
    }

    fn storage_rollback_transaction(&mut self) -> Result<(), ()> {
        self.layers.pop().ok_or(())?;
        Ok(())
    }

    fn storage_commit_transaction(&mut self) -> Result<(), ()> {
        let top = self.layers.pop().ok_or(())?;
        if let Some(parent) = self.layers.last_mut() {
            for (k, v) in top.top {
                parent.top.insert(k, v);
            }
            for (storage_key, child) in top.children {
                let ChildLayer { info, killed, writes } = child;
                let entry = parent
                    .children
                    .entry(storage_key)
                    .or_insert_with(|| ChildLayer::new(info.clone()));
                if killed {
                    entry.killed = true;
                    entry.writes.clear();
                }
                entry.writes.extend(writes);
            }
        } else {
            merge_top_into_base(&mut self.base_top, top.top);
            merge_children_into_base(&mut self.base_children, top.children);
        }
        Ok(())
    }

    fn wipe(&mut self) {}
    fn commit(&mut self) {}

    fn read_write_count(&self) -> (u32, u32, u32, u32) {
        unimplemented!("read_write_count is not supported in FuzzingExternalities")
    }

    fn reset_read_write_count(&mut self) {
        unimplemented!("reset_read_write_count is not supported in FuzzingExternalities")
    }

    fn get_whitelist(&self) -> Vec<TrackedStorageKey> {
        unimplemented!("get_whitelist is not supported in FuzzingExternalities")
    }

    fn set_whitelist(&mut self, _: Vec<TrackedStorageKey>) {
        unimplemented!("set_whitelist is not supported in FuzzingExternalities")
    }

    fn get_read_and_written_keys(&self) -> Vec<(Vec<u8>, u32, u32, bool)> {
        unimplemented!("get_read_and_written_keys is not supported in FuzzingExternalities")
    }
}

// ---------------------------------------------------------------------------
// ExtensionStore impl
// ---------------------------------------------------------------------------

impl sp_externalities::ExtensionStore for FuzzingExternalities {
    fn extension_by_type_id(&mut self, type_id: TypeId) -> Option<&mut dyn Any> {
        self.extensions.get_mut(type_id)
    }

    fn register_extension_with_type_id(
        &mut self,
        type_id: TypeId,
        extension: Box<dyn Extension>,
    ) -> Result<(), sp_externalities::Error> {
        self.extensions.register_with_type_id(type_id, extension)
    }

    fn deregister_extension_by_type_id(
        &mut self,
        type_id: TypeId,
    ) -> Result<(), sp_externalities::Error> {
        if self.extensions.deregister(type_id) {
            Ok(())
        } else {
            Err(sp_externalities::Error::ExtensionIsNotRegistered(type_id))
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn merge_top_into_base(
    base: &mut BTreeMap<StorageKey, StorageValue>,
    overlay: HashMap<StorageKey, Option<StorageValue>>,
) {
    for (k, v) in overlay {
        match v {
            Some(val) => {
                base.insert(k, val);
            }
            None => {
                base.remove(&k);
            }
        }
    }
}

fn merge_children_into_base(
    base: &mut BTreeMap<StorageKey, (ChildInfo, BTreeMap<StorageKey, StorageValue>)>,
    overlay: HashMap<StorageKey, ChildLayer>,
) {
    for (storage_key, child) in overlay {
        let ChildLayer { info, killed, writes } = child;
        if killed {
            base.remove(&storage_key);
        }
        if !writes.is_empty() {
            let entry = base.entry(storage_key).or_insert_with(|| (info, BTreeMap::new()));
            for (k, v) in writes {
                match v {
                    Some(val) => {
                        entry.1.insert(k, val);
                    }
                    None => {
                        entry.1.remove(&k);
                    }
                }
            }
        }
    }
}

/// Append a SCALE-encoded element to a SCALE-encoded `Vec<T>` stored as raw bytes.
fn append_to_storage_vec(current: &mut Vec<u8>, element: &[u8]) {
    if current.is_empty() {
        Compact(1u32).encode_to(current);
        current.extend_from_slice(element);
        return;
    }

    let Ok(Compact(old_len)) = <Compact<u32>>::decode(&mut &current[..]) else {
        // Not a valid encoded Vec – overwrite with single-element vec.
        current.clear();
        Compact(1u32).encode_to(current);
        current.extend_from_slice(element);
        return;
    };

    let old_prefix_len = Compact::<u32>::compact_len(&old_len);
    let new_len = old_len.saturating_add(1);
    let new_prefix_len = Compact::<u32>::compact_len(&new_len);

    if old_prefix_len == new_prefix_len {
        // Prefix stays the same width – update in-place and append.
        let mut buf = Vec::with_capacity(new_prefix_len);
        Compact(new_len).encode_to(&mut buf);
        current[..new_prefix_len].copy_from_slice(&buf);
        current.extend_from_slice(element);
    } else {
        // Prefix grew – rebuild.
        let items = current[old_prefix_len..].to_vec();
        current.clear();
        Compact(new_len).encode_to(current);
        current.extend_from_slice(&items);
        current.extend_from_slice(element);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sp_core::traits::Externalities;

    fn ext() -> FuzzingExternalities {
        FuzzingExternalities::default()
    }

    #[test]
    fn read_write_base() {
        let mut e = ext();
        assert_eq!(e.storage(b"k"), None);
        e.place_storage(b"k".to_vec(), Some(b"v".to_vec()));
        assert_eq!(e.storage(b"k"), Some(b"v".to_vec()));
        e.place_storage(b"k".to_vec(), None);
        assert_eq!(e.storage(b"k"), None);
    }

    #[test]
    fn layered_read_write() {
        let mut e = ext();
        e.place_storage(b"a".to_vec(), Some(b"1".to_vec()));
        e.storage_start_transaction();
        e.place_storage(b"a".to_vec(), Some(b"2".to_vec()));
        e.place_storage(b"b".to_vec(), Some(b"X".to_vec()));
        assert_eq!(e.storage(b"a"), Some(b"2".to_vec()));
        assert_eq!(e.storage(b"b"), Some(b"X".to_vec()));
        e.storage_rollback_transaction().unwrap();
        assert_eq!(e.storage(b"a"), Some(b"1".to_vec()));
        assert_eq!(e.storage(b"b"), None);
    }

    #[test]
    fn commit_merges_into_base() {
        let mut e = ext();
        e.storage_start_transaction();
        e.place_storage(b"k".to_vec(), Some(b"v".to_vec()));
        e.storage_commit_transaction().unwrap();
        assert_eq!(e.storage(b"k"), Some(b"v".to_vec()));
        assert!(e.layers.is_empty());
    }

    #[test]
    fn nested_transactions() {
        let mut e = ext();
        e.place_storage(b"k".to_vec(), Some(b"0".to_vec()));
        e.storage_start_transaction();
        e.place_storage(b"k".to_vec(), Some(b"1".to_vec()));
        e.storage_start_transaction();
        e.place_storage(b"k".to_vec(), Some(b"2".to_vec()));
        assert_eq!(e.storage(b"k"), Some(b"2".to_vec()));
        e.storage_rollback_transaction().unwrap();
        assert_eq!(e.storage(b"k"), Some(b"1".to_vec()));
        e.storage_commit_transaction().unwrap();
        assert_eq!(e.storage(b"k"), Some(b"1".to_vec()));
    }

    #[test]
    fn delete_in_layer_commits_delete() {
        let mut e = ext();
        e.place_storage(b"k".to_vec(), Some(b"v".to_vec()));
        e.storage_start_transaction();
        e.place_storage(b"k".to_vec(), None);
        assert_eq!(e.storage(b"k"), None);
        e.storage_commit_transaction().unwrap();
        assert_eq!(e.storage(b"k"), None);
    }

    #[test]
    fn rollback_without_transaction_errors() {
        let mut e = ext();
        assert!(e.storage_rollback_transaction().is_err());
        assert!(e.storage_commit_transaction().is_err());
    }

    #[test]
    fn next_storage_key_merges_base_and_layers() {
        let mut e = ext();
        e.place_storage(b"a".to_vec(), Some(b"1".to_vec()));
        e.place_storage(b"c".to_vec(), Some(b"3".to_vec()));
        e.storage_start_transaction();
        e.place_storage(b"b".to_vec(), Some(b"2".to_vec()));
        assert_eq!(e.next_storage_key(b""), Some(b"a".to_vec()));
        assert_eq!(e.next_storage_key(b"a"), Some(b"b".to_vec()));
        assert_eq!(e.next_storage_key(b"b"), Some(b"c".to_vec()));
        assert_eq!(e.next_storage_key(b"c"), None);
    }

    #[test]
    fn next_storage_key_skips_layer_deleted_keys() {
        let mut e = ext();
        e.place_storage(b"a".to_vec(), Some(b"1".to_vec()));
        e.place_storage(b"b".to_vec(), Some(b"2".to_vec()));
        e.storage_start_transaction();
        e.place_storage(b"a".to_vec(), None);
        assert_eq!(e.next_storage_key(b""), Some(b"b".to_vec()));
    }

    #[test]
    fn clear_prefix_spans_base_and_layers() {
        let mut e = ext();
        e.place_storage(b"px_1".to_vec(), Some(b"1".to_vec()));
        e.place_storage(b"py_1".to_vec(), Some(b"y".to_vec()));
        e.storage_start_transaction();
        e.place_storage(b"px_2".to_vec(), Some(b"2".to_vec()));
        let r = e.clear_prefix(b"px_", None, None);
        assert_eq!(r.backend, 2);
        assert_eq!(e.storage(b"px_1"), None);
        assert_eq!(e.storage(b"px_2"), None);
        assert_eq!(e.storage(b"py_1"), Some(b"y".to_vec()));
        // Rollback: base state restored, layer-only px_2 gone.
        e.storage_rollback_transaction().unwrap();
        assert_eq!(e.storage(b"px_1"), Some(b"1".to_vec()));
        assert_eq!(e.storage(b"px_2"), None);
    }

    #[test]
    fn storage_append_across_layers() {
        let mut e = ext();
        // First append on empty base: Compact(1) ++ 0xA
        e.storage_append(b"v".to_vec(), vec![0xA]);
        assert_eq!(e.storage(b"v"), Some(vec![1 << 2, 0xA]));

        e.storage_start_transaction();
        e.storage_append(b"v".to_vec(), vec![0xB]);
        assert_eq!(e.storage(b"v"), Some(vec![2 << 2, 0xA, 0xB]));
        // Subsequent append in same layer hits the in-place fast path.
        e.storage_append(b"v".to_vec(), vec![0xC]);
        assert_eq!(e.storage(b"v"), Some(vec![3 << 2, 0xA, 0xB, 0xC]));

        e.storage_rollback_transaction().unwrap();
        assert_eq!(e.storage(b"v"), Some(vec![1 << 2, 0xA]));
    }

    #[test]
    fn child_storage_basics() {
        let mut e = ext();
        let info = ChildInfo::new_default(b"child1");
        e.place_child_storage(&info, b"k".to_vec(), Some(b"v".to_vec()));
        assert_eq!(e.child_storage(&info, b"k"), Some(b"v".to_vec()));

        e.storage_start_transaction();
        let _ = e.kill_child_storage(&info, None, None);
        assert_eq!(e.child_storage(&info, b"k"), None);
        e.place_child_storage(&info, b"k2".to_vec(), Some(b"w".to_vec()));
        assert_eq!(e.child_storage(&info, b"k2"), Some(b"w".to_vec()));
        assert_eq!(e.child_storage(&info, b"k"), None);
        e.storage_rollback_transaction().unwrap();
        assert_eq!(e.child_storage(&info, b"k"), Some(b"v".to_vec()));
        assert_eq!(e.child_storage(&info, b"k2"), None);
    }

    #[test]
    fn child_kill_then_commit_propagates() {
        let mut e = ext();
        let info = ChildInfo::new_default(b"child1");
        e.place_child_storage(&info, b"k".to_vec(), Some(b"v".to_vec()));
        e.storage_start_transaction();
        let _ = e.kill_child_storage(&info, None, None);
        e.place_child_storage(&info, b"k2".to_vec(), Some(b"w".to_vec()));
        e.storage_commit_transaction().unwrap();
        assert_eq!(e.child_storage(&info, b"k"), None);
        assert_eq!(e.child_storage(&info, b"k2"), Some(b"w".to_vec()));
    }

    #[test]
    fn into_storage_flushes_pending_layers() {
        let mut e = ext();
        e.place_storage(b"a".to_vec(), Some(b"1".to_vec()));
        e.storage_start_transaction();
        e.place_storage(b"b".to_vec(), Some(b"2".to_vec()));
        e.storage_start_transaction();
        e.place_storage(b"c".to_vec(), Some(b"3".to_vec()));
        let storage = e.into_storage();
        assert_eq!(storage.top.get(b"a".as_slice()), Some(&b"1".to_vec()));
        assert_eq!(storage.top.get(b"b".as_slice()), Some(&b"2".to_vec()));
        assert_eq!(storage.top.get(b"c".as_slice()), Some(&b"3".to_vec()));
    }
}
