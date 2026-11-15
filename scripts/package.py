#!/usr/bin/env python3
"""Build verified source or portable handoff archives without local DBs or binaries."""
from pathlib import Path
from zipfile import ZipFile, ZIP_DEFLATED, ZipInfo
import argparse
import hashlib
import json
import stat

ROOT = Path(__file__).resolve().parents[1]
DIRS = ['contracts', 'crates', 'docs', 'evaluation', 'lyquids', 'scripts', 'tests', 'vendor', 'manual']
TOP = ['.gitignore', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'foundry.toml', 'README.md',
       'START_HERE.zh-CN.md', 'Start-TreasuryLens.command', 'Start-TreasuryLens.sh']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--portable', action='store_true', help='Include pinned LDK source and illustrated user manual.')
    args = parser.parse_args()
    output = ROOT / 'output/project'
    output.mkdir(parents=True, exist_ok=True)
    archive = output / ('TreasuryLens-完整项目.zip' if args.portable else 'TreasuryLens-source.zip')
    files = [ROOT / p for p in TOP]
    for folder in DIRS:
        files.extend(p for p in (ROOT / folder).rglob('*')
                     if p.is_file() and '__pycache__' not in p.parts and p.suffix != '.pyc')
    if args.portable:
        ldk = ROOT / '.tools/ldk'
        if not (ldk / 'LICENSE').is_file() or not (ldk / 'lyquid/Cargo.toml').is_file():
            raise SystemExit('Run bootstrap first: pinned LDK source is required for the portable archive.')
        files.extend(p for p in ldk.rglob('*') if p.is_file() and
                     not {'target', '.git', '__pycache__'}.intersection(p.relative_to(ldk).parts))
    entries = {}
    with ZipFile(archive, 'w', ZIP_DEFLATED) as z:
        def add(path, rel):
            data = path.read_bytes()
            info = ZipInfo('TreasuryLens/' + rel, __import__('time').localtime(path.stat().st_mtime)[:6])
            info.create_system = 3
            info.compress_type = ZIP_DEFLATED
            mode = 0o755 if path.stat().st_mode & 0o111 else 0o644
            info.external_attr = (stat.S_IFREG | mode) << 16
            z.writestr(info, data)
            entries[rel] = hashlib.sha256(data).hexdigest()

        for p in sorted(set(files)):
            add(p, p.relative_to(ROOT).as_posix())
        # Materialize the intentional test-assets symlink for ZIP tools on all OSes.
        assets = ROOT / 'lyquids/treasurylens/assets'
        for p in assets.rglob('*'):
            if p.is_file():
                rel = 'lyquids/treasurylens-test/assets/' + p.relative_to(assets).as_posix()
                if rel not in entries:
                    add(p, rel)
        if args.portable:
            manual = ROOT / 'output/pdf/TreasuryLens_网站全功能图文手册.pdf'
            if 'manual/TreasuryLens_网站全功能图文手册.pdf' not in entries:
                add(manual, 'manual/TreasuryLens_网站全功能图文手册.pdf')
        z.writestr('TreasuryLens/SOURCE_MANIFEST.json', json.dumps({
            'kind': 'source-file-sha256', 'portable': args.portable, 'files': entries
        }, indent=2, ensure_ascii=False) + '\n')
    with ZipFile(archive) as z:
        assert z.testzip() is None, 'Archive CRC verification failed'
        assert len(z.namelist()) == len(set(z.namelist())), 'Duplicate archive paths'
        for rel, digest in entries.items():
            assert hashlib.sha256(z.read('TreasuryLens/' + rel)).hexdigest() == digest
        assert 'TreasuryLens/lyquids/treasurylens-test/assets/index.html' in z.namelist()
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_suffix('.zip.sha256').write_text(digest + '  ' + archive.name + '\n')
    print(f'{archive}\n{len(entries)} files; {archive.stat().st_size:,} bytes; SHA256 {digest}')


if __name__ == '__main__':
    main()
