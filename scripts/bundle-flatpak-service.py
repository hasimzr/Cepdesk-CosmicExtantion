#!/usr/bin/env python3
"""Bundle the service ELF closure, including its loader and image plugins.

Run inside the Flatpak SDK after cargo build. Never use host prebuilts in the
store manifest: the service and its libraries must match the target architecture.
"""
import argparse
import pathlib
import re
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=pathlib.Path, required=True)
parser.add_argument('--output', type=pathlib.Path, required=True)
args = parser.parse_args()
out = args.output
library_sources = set()
for name in ['bin', 'lib', 'loaders']:
    (out / name).mkdir(parents=True, exist_ok=True)


def copy_elf(source, destination):
    shutil.copy2(source, destination)
    result = subprocess.run(['ldd', str(source)], text=True, capture_output=True, check=True)
    if 'not found' in result.stdout:
        raise RuntimeError(f'Unresolved dependency for {source}: {result.stdout}')
    for dependency in re.findall(r'(?:=>\s+|^\s*)(/[^\s]+)', result.stdout, re.MULTILINE):
        path = pathlib.Path(dependency)
        library_sources.add(path.resolve())
        shutil.copy2(path, out / 'lib' / path.name)
        if path.name.startswith('ld-linux') or path.name.startswith('ld-'):
            (out / 'loader-name').write_text(path.name + '\n')


copy_elf(args.binary, out / 'bin/cepdesk')
query = shutil.which('gdk-pixbuf-query-loaders')
if not query:
    query = next(pathlib.Path('/usr/lib').glob('**/gdk-pixbuf-query-loaders'), None)
if not query:
    raise RuntimeError('gdk-pixbuf-query-loaders is required in the SDK')
copy_elf(pathlib.Path(query), out / 'bin/gdk-pixbuf-query-loaders')
plugins = []
elf_header = args.binary.read_bytes()[:20]
for prefix in ['/usr/lib', '/app/lib']:
    for path in pathlib.Path(prefix).glob('**/gdk-pixbuf-2.0/**/loaders/*.so'):
        header = path.read_bytes()[:20]
        if header[4:6] == elf_header[4:6] and header[18:20] == elf_header[18:20]:
            plugins.append(path)
if not plugins:
    raise RuntimeError('No GdkPixbuf image loaders found in SDK/BaseApp')
for plugin in plugins:
    copy_elf(plugin, out / 'loaders' / plugin.name)
if not (out / 'loader-name').is_file():
    raise RuntimeError('ELF dynamic loader was not bundled')

# Retain the runtime libraries' notices alongside the private ELF bundle.
licenses = out / 'licenses'
licenses.mkdir(exist_ok=True)
for prefix in ['/usr', '/app']:
    source = pathlib.Path(prefix) / 'share/licenses'
    if source.is_dir():
        shutil.copytree(source, licenses / source.parent.parent.name, dirs_exist_ok=True)
if shutil.which('dpkg-query'):
    for source in library_sources:
        result = subprocess.run(['dpkg-query', '-S', str(source)], capture_output=True, text=True)
        if result.returncode == 0:
            for line in result.stdout.splitlines():
                package = line.split(': ', 1)[0].split(':', 1)[0]
                copyright_file = pathlib.Path('/usr/share/doc') / package / 'copyright'
                if copyright_file.is_file():
                    shutil.copy2(copyright_file, licenses / (package + '.copyright'))
common = pathlib.Path('/usr/share/common-licenses')
if common.is_dir():
    shutil.copytree(common, licenses / 'common', dirs_exist_ok=True)
if pathlib.Path('LICENSE').is_file():
    shutil.copy2('LICENSE', licenses / 'CepDesk.LICENSE')
