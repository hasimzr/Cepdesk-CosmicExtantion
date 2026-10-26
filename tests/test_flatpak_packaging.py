"""Host setup and store preparation without touching the current user session."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]


class FlatpakPackagingTests(unittest.TestCase):
    def test_first_launch_and_preservation(self):
        with tempfile.TemporaryDirectory(prefix='cepdesk setup ') as temporary:
            root = Path(temporary)
            home = root / 'home'
            home.mkdir()
            binary_dir = root / 'bin'
            binary_dir.mkdir()
            log = root / 'calls'
            fake = binary_dir / 'systemctl'
            fake.write_text('''#!/bin/sh
printf '%s\\n' "$*" >> "$TEST_LOG"
case "$*" in
 *FragmentPath*) printf '%s\\n' "$TEST_FRAGMENT";;
esac
''')
            fake.chmod(0o755)
            env = dict(os.environ, HOME=str(home),
                       XDG_CONFIG_HOME=str(home / 'custom config'),
                       PATH=str(binary_dir) + ':' + os.environ['PATH'],
                       TEST_LOG=str(log), TEST_FRAGMENT='')
            unit = home / 'custom config/systemd/user/cepdesk.service'

            def run():
                log.write_text('')
                subprocess.run(['sh', str(ROOT / 'packaging/flatpak/helpers/cepdesk-host-setup')],
                               env=env, check=True)
                return log.read_text()

            calls = run()
            self.assertIn('--command=cepdesk-host-launch', unit.read_text())
            self.assertIn('daemon-reload', calls)
            self.assertIn('enable cepdesk.service', calls)
            self.assertEqual(unit.stat().st_mode & 0o777, 0o600)
            self.assertEqual(run(), '')  # Existing disabled unit stays disabled.
            unit.write_text('[Service]\nExecStart=/usr/bin/cepdesk serve\n')
            self.assertEqual(run(), '')
            self.assertIn('/usr/bin/cepdesk', unit.read_text())
            unit.unlink()
            unit.symlink_to('/dev/null')
            self.assertEqual(run(), '')
            self.assertTrue(unit.is_symlink())
            unit.unlink()
            env['TEST_FRAGMENT'] = '/usr/lib/systemd/user/cepdesk.service'
            run()
            self.assertFalse(unit.exists())

    def test_submission_pins_service_and_applet_to_same_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in ['scripts/prepare-cosmic-store.py',
                             'packaging/flatpak/dev.cepdesk.Applet.json.in',
                             'resources/dev.cepdesk.Applet.metainfo.xml']:
                destination = root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / relative, destination)
            for name in ['cargo-sources.json', 'service-cargo-sources.json']:
                (root / name).write_text('[]')
            sha = 'a' * 40
            subprocess.run(['python3', str(root / 'scripts/prepare-cosmic-store.py'),
                            '--repository', 'https://example.invalid/CepDesk.git', '--commit', sha,
                            '--cargo-sources', str(root / 'cargo-sources.json'),
                            '--service-cargo-sources', str(root / 'service-cargo-sources.json')],
                           check=True, capture_output=True)
            output = root / 'release/store-submission/app/dev.cepdesk.Applet'
            manifest = json.loads((output / 'dev.cepdesk.Applet.json').read_text())
            self.assertEqual([m['name'] for m in manifest['modules']],
                             ['cepdesk-service', 'cepdesk-applet'])
            for module in manifest['modules']:
                self.assertEqual(module['sources'][0]['commit'], sha)
                self.assertEqual(module['sources'][0]['url'], 'https://example.invalid/CepDesk.git')
            self.assertIn('--talk-name=org.freedesktop.Flatpak', manifest['finish-args'])
            self.assertTrue((output / 'service-cargo-sources.json').is_file())
            self.assertIn('service/Cargo.toml', manifest['modules'][0]['build-commands'][0])
            metadata = ET.parse(root / 'release/dev.cepdesk.Applet.metainfo.xml').getroot()
            homepages = metadata.findall("url[@type='homepage']")
            self.assertEqual(len(homepages), 1)
            self.assertEqual(homepages[0].text, 'https://cepdesk.hasimozer.com')
            self.assertEqual(metadata.find("url[@type='vcs-browser']").text,
                             'https://example.invalid/CepDesk')
            if shutil.which('appstreamcli'):
                subprocess.run(['appstreamcli', 'validate', '--no-net',
                                str(root / 'release/dev.cepdesk.Applet.metainfo.xml')],
                               check=True, capture_output=True)


if __name__ == '__main__':
    unittest.main()
