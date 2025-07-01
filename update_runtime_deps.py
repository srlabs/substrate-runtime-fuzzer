#!/usr/bin/env python3
import sys
import requests
import re
from pathlib import Path
import tempfile
import subprocess

def main() -> None:
    if len(sys.argv) != 3:
        sys.stderr.write("Usage: update_runtime_deps.py <tag> <runtime>\n")
        sys.exit(1)

    version = sys.argv[1]
    runtime = sys.argv[2]

    cargo_lock_url = f"https://raw.githubusercontent.com/polkadot-fellows/runtimes/{version}/Cargo.lock"
    target_cargo = Path(f"runtimes/{runtime}/Cargo.toml")
    
    try:
        # Download and parse Cargo.lock
        response = requests.get(cargo_lock_url)
        response.raise_for_status()
        
        # Extract package versions from lock file
        versions = {}
        current_pkg = {}
        for line in response.text.split('\n')[4:]:
            line = line.strip()
            if line.startswith('[[package]]'):
                if current_pkg.get('name') and current_pkg.get('version'):
                    if current_pkg['name'] != 'ziggy':
                        versions[current_pkg['name']] = current_pkg['version']
                current_pkg = {}
            elif line.startswith('name ='):
                current_pkg['name'] = line.split('"')[1]
            elif line.startswith('version ='):
                current_pkg['version'] = line.split('"')[1]
        
        # Capture last package entry
        if current_pkg.get('name') and current_pkg.get('version') and current_pkg['name'] != 'ziggy':
            versions[current_pkg['name']] = current_pkg['version']

        # Verify target file exists
        if not target_cargo.exists():
            sys.stderr.write(f"Error: Target file not found - {target_cargo}\n")
            sys.exit(1)

        # Process Cargo.toml line-by-line
        updated_lines = []
        in_deps_section = False
        updated_count = 0
        
        with open(target_cargo, 'r') as f:
            for line in f:
                line = line.rstrip('\n')
                
                # Detect relevant sections
                if line.startswith(('[dependencies]', '[dev-dependencies]', '[build-dependencies]')):
                    in_deps_section = True
                    updated_lines.append(line)
                    continue
                elif line.startswith('['):
                    in_deps_section = False
                    updated_lines.append(line)
                    continue
                    
                if in_deps_section:
                    # Match dependency lines - handle various formats
                    match = re.match(r'^(\s*)([a-zA-Z0-9_-]+)\s*=\s*(.*)', line)
                    if match:
                        indent, pkg_name, rest = match.groups()
                        # Determine which package name to look up in versions
                        lookup_name = pkg_name
                        if 'package =' in rest:
                            # Extract the actual package name from package = "name"
                            package_match = re.search(r'package\s*=\s*"([^"]+)"', rest)
                            if package_match:
                                lookup_name = package_match.group(1)
                        
                        if lookup_name in versions:
                            # Parse the existing dependency format
                            rest = rest.strip()
                            
                            # Handle git dependencies - update tag but don't add version
                            if 'git =' in rest:
                                # Update the tag field to match the version
                                new_rest = re.sub(r'tag\s*=\s*"[^"]*"', f'tag = "{version}"', rest)
                                new_line = f'{indent}{pkg_name} = {new_rest}'
                                updated_lines.append(new_line)
                                updated_count += 1
                                continue
                            
                            # Check if it's already a table format
                            if rest.startswith('{'):
                                # Update version in existing table format
                                # Replace version field while preserving other fields
                                new_rest = re.sub(r'version\s*=\s*"[^"]*"', f'version = "{versions[lookup_name]}"', rest)
                                # If no version field existed, add it
                                if 'version =' not in rest:
                                    # Insert version after opening brace
                                    new_rest = re.sub(r'^\{\s*', f'{{ version = "{versions[lookup_name]}", ', rest)
                                new_line = f'{indent}{pkg_name} = {new_rest}'
                            else:
                                # Simple string format or other format - convert to table with version and default-features
                                new_line = f'{indent}{pkg_name} = {{ version = "{versions[lookup_name]}", default-features = false }}'
                            
                            updated_lines.append(new_line)
                            updated_count += 1
                            continue
                            
                updated_lines.append(line)

        # Write changes atomically
        with tempfile.NamedTemporaryFile(mode='w', delete=False) as tmp_file:
            tmp_file.write('\n'.join(updated_lines))
            tmp_path = tmp_file.name
            
        subprocess.run(['mv', tmp_path, str(target_cargo)], check=True)
        
        print(f"Successfully updated {target_cargo}")

    except requests.RequestException as e:
        sys.stderr.write(f"Error downloading Cargo.lock: {e}\n")
        sys.exit(1)
    except Exception as e:
        sys.stderr.write(f"Error processing files: {e}\n")
        sys.exit(1)

if __name__ == "__main__":
    main()