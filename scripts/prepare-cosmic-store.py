#!/usr/bin/env python3
"""Render a COSMIC-repository submission; never publishes or opens a PR."""
import argparse
import json
import pathlib
import re
import shutil
import xml.etree.ElementTree as ET

root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--repository', required=True, help='Public HTTPS Git source URL')
parser.add_argument('--commit', required=True, help='Full immutable upstream commit SHA')
parser.add_argument('--cargo-sources', type=pathlib.Path, required=True, help='flatpak-cargo-generator output for Cargo.lock')
parser.add_argument('--service-cargo-sources', type=pathlib.Path, required=True, help='flatpak-cargo-generator output for service/Cargo.lock')
args = parser.parse_args()
if not args.repository.startswith('https://') or '@' in args.repository:
    parser.error('Use a public HTTPS repository URL without embedded credentials.')
if not re.fullmatch('[0-9a-fA-F]{40}', args.commit):
    parser.error('A full 40-character source commit is required.')
if not args.cargo_sources.is_file():
    parser.error('Generate cargo-sources.json from the final lockfile first.')
if not args.service_cargo_sources.is_file():
    parser.error('Generate service-cargo-sources.json from service/Cargo.lock first.')
json.loads(args.cargo_sources.read_text())
json.loads(args.service_cargo_sources.read_text())
output = root / 'release/store-submission/app/io.github.hasimzr.CepDesk'
output.mkdir(parents=True, exist_ok=True)
manifest = json.loads((root / 'packaging/flatpak/io.github.hasimzr.CepDesk.json.in').read_text())
for module in manifest['modules']:
    source = module['sources'][0]
    source['url'] = args.repository
    source['commit'] = args.commit.lower()
(output / 'io.github.hasimzr.CepDesk.json').write_text(json.dumps(manifest, indent=2) + '\n')
shutil.copy2(args.cargo_sources, output / 'cargo-sources.json')
shutil.copy2(args.service_cargo_sources, output / 'service-cargo-sources.json')
# Metadata belongs upstream, not in the COSMIC submission. This prepared sibling
# is reviewed before replacing upstream metadata and creating the final release.
metadata = ET.parse(root / 'resources/io.github.hasimzr.CepDesk.metainfo.xml')
component = metadata.getroot()
# Preserve the product homepage and update the source URL without duplicates.
for existing in list(component.findall("url[@type='vcs-browser']")):
    component.remove(existing)
ET.SubElement(component, 'url', {'type': 'vcs-browser'}).text = args.repository.removesuffix('.git')
ET.indent(metadata, space='  ')
metadata.write(root / 'release/io.github.hasimzr.CepDesk.metainfo.xml', encoding='UTF-8', xml_declaration=True)
print(output)
print('Before submission: commit public metadata/screenshots upstream, regenerate with that final commit, run flatpak-builder and validate AppStream. No files have been published.')
