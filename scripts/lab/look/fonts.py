"""Make the lab's private font copy (lab-browser.md, 7)."""
import argparse
import os
from pathlib import Path
import shutil
import sys
from runner import canonical, store
from runner.records import Refused

FACES = ("DejaVuSans.ttf", "DejaVuSans-Bold.ttf", "DejaVuSans-Oblique.ttf", "DejaVuSans-BoldOblique.ttf",
         "DejaVuSansMono.ttf", "DejaVuSansMono-Bold.ttf", "DejaVuSansMono-Oblique.ttf", "DejaVuSansMono-BoldOblique.ttf",
         "DejaVuSerif.ttf", "DejaVuSerif-Bold.ttf", "DejaVuSerif-Italic.ttf", "DejaVuSerif-BoldItalic.ttf")
REGULAR = {"DejaVuSans.ttf", "DejaVuSansMono.ttf", "DejaVuSerif.ttf"}


def main(argv=None):
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", metavar="SOURCE", type=Path)
    parser.add_argument("directory", metavar="DIR", type=Path)
    args = parser.parse_args(argv)
    made = False
    try:
        if not args.source.is_absolute():
            raise Refused(str(args.source), "expected an absolute source directory path")
        if os.path.lexists(args.directory):
            raise Refused(str(args.directory), "directory exists")
        fd = store.checked_directory(args.source)
        os.close(fd)
        files = {}
        for name in FACES:
            try:
                files[name], _ = store.read_regular(args.source / name, allow_hardlinks=True)
            except FileNotFoundError:
                if name in REGULAR:
                    raise Refused(name, "required regular family face is missing") from None
        files["aliases.conf"] = Path(__file__).with_name("aliases.conf").read_bytes()
        args.directory.mkdir(mode=0o700)
        made = True
        for name, data in files.items():
            with (args.directory / name).open("xb") as stream:
                stream.write(data)
                os.fchmod(stream.fileno(), 0o400)
        tree = canonical.tree_digest(store.walk(args.directory))
        args.directory.chmod(0o500)
        sys.stdout.buffer.write(canonical.record_bytes({"path": str(args.directory), "tree": tree}))
        return 0
    except (OSError, Refused) as error:
        if made:
            args.directory.chmod(0o700)
            shutil.rmtree(args.directory)
        print(error, file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
