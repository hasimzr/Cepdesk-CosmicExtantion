"""Verify packaging from an independent checkout, without sibling projects/tools."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class StandaloneRepositoryTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which('dpkg-deb'), 'dpkg-deb required')
    def test_debian_package_from_independent_checkout(self):
        with tempfile.TemporaryDirectory(prefix='cepdesk-standalone-') as temporary:
            directory = Path(temporary)
            checkout = directory / 'renamed desktop repository'
            shutil.copytree(ROOT, checkout, ignore=shutil.ignore_patterns(
                'target', 'release', '.tools', '__pycache__', '.git'))
            tools = directory / 'test-bin'
            tools.mkdir()
            cargo = tools / 'cargo'
            # Stub compilation only: exercise the real staging/dpkg-deb logic.
            cargo.write_text(f'#!{sys.executable}\n' + '''import pathlib, sys, tomllib
args = sys.argv[1:]
assert args[0] == 'build', args
manifest = pathlib.Path(args[args.index('--manifest-path') + 1])
assert manifest.is_file(), manifest
name = tomllib.loads(manifest.read_text())['package']['name']
binary = manifest.parent / 'target/release' / name
binary.parent.mkdir(parents=True, exist_ok=True)
binary.write_text('#!/bin/sh\\nexit 0\\n')
binary.chmod(0o755)
''')
            cargo.chmod(0o755)
            environment = dict(os.environ, PATH=str(tools) + ':' + os.environ['PATH'])
            subprocess.run(['bash', str(checkout / 'scripts/build-cosmic-package.sh')],
                           cwd=directory, env=environment, check=True, capture_output=True)
            package, = (checkout / 'release').glob('*.deb')
            extracted = directory / 'extracted'
            subprocess.run(['dpkg-deb', '-x', str(package), str(extracted)], check=True)
            for name in ['cepdesk', 'cepdesk-applet']:
                self.assertTrue((extracted / 'usr/bin' / name).is_file())
                self.assertTrue((checkout / 'release' / name).is_file())
            self.assertEqual((extracted / 'usr/share/doc/cepdesk/copyright').read_bytes(),
                             (ROOT / 'LICENSE').read_bytes())
            self.assertFalse((checkout / '.tools').exists())

    @unittest.skipUnless(shutil.which('git'), 'git required')
    def test_gitignore_preserves_sources_and_excludes_outputs(self):
        with tempfile.TemporaryDirectory(prefix='cepdesk-gitignore-') as temporary:
            checkout = Path(temporary)
            shutil.copy2(ROOT / '.gitignore', checkout / '.gitignore')
            subprocess.run(['git', 'init', '--quiet', str(checkout)], check=True)
            for name in ['target/debug/cepdesk-applet', 'service/target/release/cepdesk',
                         'release/test.deb', '.tools/cargo/bin/cargo', '.test-state/config/password.json',
                         'server.key', 'identity.json', 'cepdesk.sqlite3', 'test.db', 'tests/__pycache__/test.pyc']:
                result = subprocess.run(['git', '-C', str(checkout), 'check-ignore', '--no-index', '-q', name])
                self.assertEqual(result.returncode, 0, name)
            for name in ['Cargo.lock', 'service/Cargo.lock', 'LICENSE', 'README.md',
                         'src/main.rs', 'service/src/main.rs', 'resources/dev.cepdesk.Applet.png']:
                result = subprocess.run(['git', '-C', str(checkout), 'check-ignore', '--no-index', '-q', name])
                self.assertEqual(result.returncode, 1, name)


if __name__ == '__main__':
    unittest.main()
