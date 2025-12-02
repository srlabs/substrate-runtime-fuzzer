#!/usr/bin/env python3
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import requests


def load_versions(cargo_lock_url: str) -> dict[str, str]:
    response = requests.get(cargo_lock_url)
    response.raise_for_status()

    versions: dict[str, str] = {}
    current_pkg: dict[str, str] = {}

    for line in response.text.splitlines()[4:]:
        line = line.strip()
        if line.startswith('[[package]]'):
            if current_pkg.get('name') and current_pkg.get('version') and current_pkg['name'] != 'ziggy':
                versions[current_pkg['name']] = current_pkg['version']
            current_pkg = {}
            continue

        if line.startswith('name ='):
            current_pkg['name'] = line.split('"')[1]
        elif line.startswith('version ='):
            current_pkg['version'] = line.split('"')[1]

    if current_pkg.get('name') and current_pkg.get('version') and current_pkg['name'] != 'ziggy':
        versions[current_pkg['name']] = current_pkg['version']

    return versions


def update_workspace_dependencies(target_cargo: Path, tag: str, versions: dict[str, str]) -> int:
    updated_lines = []
    in_deps_section = False
    updated_count = 0

    with open(target_cargo, 'r') as f:
        for raw_line in f:
            line = raw_line.rstrip('\n')
            stripped = line.strip()

            if stripped.startswith('[workspace.dependencies]'):
                in_deps_section = True
                updated_lines.append(line)
                continue
            if stripped.startswith('[') and not stripped.startswith('[workspace.dependencies]'):
                in_deps_section = False
                updated_lines.append(line)
                continue

            if not in_deps_section:
                updated_lines.append(line)
                continue

            match = re.match(r'^(\s*)([a-zA-Z0-9_-]+)\s*=\s*(.*)', line)
            if not match:
                updated_lines.append(line)
                continue

            indent, pkg_name, rest = match.groups()
            rest_stripped = rest.strip()

            lookup_name = pkg_name
            package_match = re.search(r'package\s*=\s*"([^"]+)"', rest_stripped)
            if package_match:
                lookup_name = package_match.group(1)

            if 'git =' in rest_stripped:
                if 'tag' in rest_stripped:
                    new_rest = re.sub(r'tag\s*=\s*"[^"]*"', f'tag = "{tag}"', rest_stripped)
                else:
                    if rest_stripped.startswith('{') and rest_stripped.endswith('}'):
                        new_rest = rest_stripped[:-1].rstrip()
                        if not new_rest.endswith('{'):
                            new_rest += ','
                        new_rest += f' tag = "{tag}" }}'
                    else:
                        new_rest = f'{{ {rest_stripped}, tag = "{tag}" }}'
                updated_lines.append(f'{indent}{pkg_name} = {new_rest}')
                updated_count += 1
                continue

            if lookup_name in versions:
                if rest_stripped.startswith('{'):
                    if 'version' in rest_stripped:
                        new_rest = re.sub(r'version\s*=\s*"[^"]*"', f'version = "{versions[lookup_name]}"', rest_stripped)
                    else:
                        new_rest = re.sub(r'^\{\s*', f'{{ version = "{versions[lookup_name]}", ', rest_stripped)
                    updated_lines.append(f'{indent}{pkg_name} = {new_rest}')
                else:
                    updated_lines.append(f'{indent}{pkg_name} = "{versions[lookup_name]}"')

                updated_count += 1
                continue

            updated_lines.append(line)

    with tempfile.NamedTemporaryFile(mode='w', delete=False) as tmp_file:
        tmp_file.write('\n'.join(updated_lines))
        tmp_path = tmp_file.name

    subprocess.run(['mv', tmp_path, str(target_cargo)], check=True)
    return updated_count


def main() -> None:
    if len(sys.argv) != 2:
        sys.stderr.write("Usage: update_runtime_deps.py <tag>\n")
        sys.exit(1)

    tag = sys.argv[1]
    target_cargo = Path("runtimes/Cargo.toml")
    cargo_lock_url = f"https://raw.githubusercontent.com/polkadot-fellows/runtimes/{tag}/Cargo.lock"

    try:
        if not target_cargo.exists():
            sys.stderr.write(f"Error: Target file not found - {target_cargo}\n")
            sys.exit(1)

        versions = load_versions(cargo_lock_url)
        updated_count = update_workspace_dependencies(target_cargo, tag, versions)
        print(f"Successfully updated {target_cargo} ({updated_count} entries)")

    except requests.RequestException as e:
        sys.stderr.write(f"Error downloading Cargo.lock: {e}\n")
        sys.exit(1)
    except Exception as e:
        sys.stderr.write(f"Error processing files: {e}\n")
        sys.exit(1)


if __name__ == "__main__":
    main()
