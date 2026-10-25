#!/usr/bin/env python3
"""Build an isolated keyboard candidate without installing or registering it."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import subprocess
import uuid

ROOT = Path(__file__).resolve().parent.parent

def qa_identity(app):
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    bundle_id = info.get('CFBundleIdentifier', '')
    if not re.fullmatch(r'dev\.facundo\.ReadyForAirStudio\.verification\.[A-Za-z0-9]+', bundle_id):
        raise ValueError('Identity reuse requires an isolated ACA QA app')
    state = Path(info.get('ACAIsolatedDataRoot', ''))
    if state.parent != Path('/private/tmp') or not re.fullmatch(r'aca-qa-[A-Za-z0-9-]+', state.name):
        raise ValueError('The QA app must have an isolated data root under /private/tmp')
    if not state.is_dir() or state.is_symlink():
        raise ValueError('The existing QA data root must be a directory, not a symlink')
    if not isinstance(info.get('CFBundleName'), str) or not info['CFBundleName']:
        raise ValueError('The QA app must have a bundle name')
    return {key: info[key] for key in ('CFBundleIdentifier', 'CFBundleName', 'ACAIsolatedDataRoot')}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reuse-identity-from', type=Path,
                        help='Stage an update with the same QA identity and data root; does not replace the source app')
    args = parser.parse_args()
    reused_app = args.reuse_identity_from.resolve() if args.reuse_identity_from else None
    reused_identity = qa_identity(reused_app) if reused_app else None
    env = os.environ.copy()
    identity = env.get('READY_AIR_SIGN_IDENTITY', '-')
    if reused_identity and identity == '-':
        parser.error('Identity reuse requires READY_AIR_SIGN_IDENTITY for the existing QA signing certificate')
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:8]
    run = ROOT / 'diagnostics' / 'keyboard-mvp' / stamp
    dist = ROOT / 'dist' / 'keyboard-qa' / stamp
    state = Path(reused_identity['ACAIsolatedDataRoot']) if reused_identity else Path('/private/tmp') / ('aca-qa-' + stamp)
    run.mkdir(parents=True)
    if not reused_identity:
        state.mkdir(mode=0o700)
    env.update(ACA_VIRTUAL_KEYBOARD='1', READY_AIR_DIST=str(dist), ACA_BUILD_ID=stamp)
    with (run / 'build.log').open('w') as log:
        subprocess.run(['sh', 'script/build_unified.sh'], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    app = dist / 'ACA.app'
    info = app / 'Contents/Info.plist'
    plist = plistlib.loads(info.read_bytes())
    plist.update(reused_identity or {
        'CFBundleIdentifier': plist['CFBundleIdentifier'] + '.verification.' + stamp.replace('-', ''),
        'CFBundleName': 'ACA Keyboard QA ' + stamp,
        'ACAIsolatedDataRoot': str(state),
    })
    info.write_bytes(plistlib.dumps(plist))
    daemon = app / 'Contents/Library/LaunchDaemons/dev.facundo.ReadyForAirCapture.plist'
    daemon_plist = plistlib.loads(daemon.read_bytes())
    daemon_plist['AssociatedBundleIdentifiers'] = [plist['CFBundleIdentifier']]
    daemon.write_bytes(plistlib.dumps(daemon_plist))
    # XPC authenticates this code identifier; the GUI bundle ID stays unique for QA isolation.
    code_id = 'dev.facundo.ReadyForAirStudio' if identity != '-' else plist['CFBundleIdentifier']
    subprocess.run(['codesign', '--force', '--options', 'runtime', '--timestamp=none',
                    '--identifier', code_id, '--sign', identity, str(app)], check=True)
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
    executable = app / 'Contents/MacOS/ReadyForAirStudio'
    status = subprocess.check_output([str(executable), '--service-status'], text=True)
    (run / 'service-status.txt').write_text(status)
    subprocess.run([str(executable), '--verify-capture'], check=True)
    paths = [ROOT / p for p in ['Cargo.toml', 'Cargo.lock', 'build.rs', 'script/build_unified.sh', 'script/keyboard_qa.py']]
    paths += sorted((ROOT / 'src').rglob('*.rs')) + sorted((ROOT / 'native').glob('*'))
    config = Path.home() / 'Library/Application Support/ReadyForAirLab/config.json'
    manifest = dict(app=str(app), state=str(state), build_id=stamp,
        source={str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths if p.is_file()},
        user_config_sha256=hashlib.sha256(config.read_bytes()).hexdigest() if config.exists() else None,
        signing='ad-hoc' if identity == '-' else 'certificate',
        code_identifier=code_id,
        reuses_qa_identity_from=str(reused_app) if reused_app else None,
        variant='managed-reader virtual-keyboard; isolated embedded data root; no service registration')
    (run / 'run.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(run)
    print(app)
    if reused_app:
        print('Staged update only. Keep the existing QA app in place until replacement is authorized.')

if __name__ == '__main__':
    main()
