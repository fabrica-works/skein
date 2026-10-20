"""Filesystem effects for private, immutable inputs (lab.md, 2.4 and 3)."""
from contextlib import contextmanager
import fcntl
import hashlib
import os
from pathlib import Path
import stat
import subprocess
import secrets
from . import canonical
from .records import NAME, Refused, digest_value, relative, subject_refusals

DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
FILE_FLAGS = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC


def file_digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def checked_directory(path):
    """Open each absolute component without following a link."""
    path = os.path.abspath(path)
    fd = os.open("/", DIR_FLAGS)
    try:
        for component in Path(path).parts[1:]:
            child = os.open(component, DIR_FLAGS, dir_fd=fd)
            os.close(fd)
            fd = child
        return fd
    except BaseException:
        os.close(fd)
        raise


def _read_at(fd, name, *, allow_hardlinks=False):
    opened = os.open(name, FILE_FLAGS, dir_fd=fd)
    try:
        metadata = os.fstat(opened)
        if not stat.S_ISREG(metadata.st_mode) or (not allow_hardlinks and metadata.st_nlink != 1):
            raise Refused(name, "expected a regular file with one link")
        with os.fdopen(os.dup(opened), "rb") as stream:
            data = stream.read()
        after = os.fstat(opened)
        if (metadata.st_size, metadata.st_mtime_ns, metadata.st_ctime_ns) != (after.st_size, after.st_mtime_ns, after.st_ctime_ns):
            raise Refused(name, "file changed while read")
        return data, bool(metadata.st_mode & stat.S_IXUSR)
    finally:
        os.close(opened)


def read_regular(path, *, allow_hardlinks=False):
    fd = checked_directory(os.path.dirname(os.path.abspath(path)))
    try:
        return _read_at(fd, os.path.basename(path), allow_hardlinks=allow_hardlinks)
    finally:
        os.close(fd)


def snapshot(directory, *, exclude=()):
    """Read a tree as file bytes; every walked component is opened no-follow."""
    root = checked_directory(directory)
    try:
        return _snapshot_fd(root, exclude)
    finally:
        os.close(root)


def _snapshot_fd(root, exclude=()):
    result = []

    def visit(fd, prefix):
        for name in sorted(os.listdir(fd)):
            path = prefix + name
            if any(path == e or path.startswith(e + "/") for e in exclude):
                continue
            try:
                path.encode("utf-8", "strict")
            except UnicodeError as error:
                raise Refused(path, "file name is not UTF-8") from error
            info = os.stat(name, dir_fd=fd, follow_symlinks=False)
            if stat.S_ISDIR(info.st_mode):
                child = os.open(name, DIR_FLAGS, dir_fd=fd)
                try:
                    visit(child, path + "/")
                finally:
                    os.close(child)
            elif stat.S_ISREG(info.st_mode):
                data, executable = _read_at(fd, name)
                result.append({"path": path, "data": data, "exec": executable})
            else:
                raise Refused(path, "links and special files are forbidden")
    visit(root, "")
    return result


def entries(files):
    return canonical.manifest({"path": f["path"], "bytes": len(f["data"]),
                               "sha256": file_digest(f["data"]), "exec": f["exec"]}
                              for f in files)["entries"]


def walk(directory, *, exclude=()):
    return entries(snapshot(directory, exclude=exclude))


def _private(fd, member):
    info = os.fstat(fd)
    if info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise Refused(member, "directory must be owned by this user and private")


class Lab:
    def __init__(self, root):
        self.root = Path(os.path.abspath(root))
        parent = checked_directory(self.root.parent)
        try:
            try:
                os.mkdir(self.root.name, 0o700, dir_fd=parent)
            except FileExistsError:
                pass
            self.fd = os.open(self.root.name, DIR_FLAGS, dir_fd=parent)
        finally:
            os.close(parent)
        try:
            _private(self.fd, "lab")
        except BaseException:
            os.close(self.fd)
            raise

    def close(self):
        os.close(self.fd)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def directory(self, path, *, create=False):
        if not relative(path):
            raise Refused(path, "expected a contained relative path")
        fd = os.dup(self.fd)
        try:
            for part in path.split("/"):
                if create:
                    try:
                        os.mkdir(part, 0o700, dir_fd=fd)
                    except FileExistsError:
                        pass
                child = os.open(part, DIR_FLAGS, dir_fd=fd)
                os.close(fd)
                fd = child
                _private(fd, path)
            return fd
        except BaseException:
            os.close(fd)
            raise

    @contextmanager
    def lock(self):
        fd = os.open("lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK, 0o600, dir_fd=self.fd)
        try:
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077 or info.st_nlink != 1:
                raise Refused("lab.lock", "lock must be a private owned regular file")
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise Refused("lab.lock", "another lab writer holds the lock") from error
            yield
        finally:
            os.close(fd)

    def _parent(self, path, create=False):
        if not relative(path):
            raise Refused(path, "expected a contained relative path")
        parts = path.rsplit("/", 1)
        return (self.directory(parts[0], create=create), parts[1]) if len(parts) == 2 else (os.dup(self.fd), path)

    def read(self, path):
        fd, name = self._parent(path)
        try:
            return _read_at(fd, name)[0]
        finally:
            os.close(fd)

    def write_once(self, path, data, *, executable=False):
        """Publish complete bytes via an exclusive hard link; verify a collision."""
        fd, name = self._parent(path, create=True)
        temporary = None
        try:
            candidate = ".input-" + secrets.token_hex(16)
            tempfd = os.open(candidate, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=fd)
            temporary = candidate
            with os.fdopen(tempfd, "wb") as stream:
                stream.write(data)
                stream.flush()
                os.fchmod(stream.fileno(), 0o500 if executable else 0o400)
                os.fsync(stream.fileno())
            try:
                os.link(temporary, name, src_dir_fd=fd, dst_dir_fd=fd, follow_symlinks=False)
            except FileExistsError:
                info = os.stat(name, dir_fd=fd, follow_symlinks=False)
                if info.st_uid != os.getuid() or stat.S_IMODE(info.st_mode) != (0o500 if executable else 0o400):
                    raise Refused(path, "immutable file mode or ownership differs")
                previous, old_exec = _read_at(fd, name)
                if previous != data or old_exec != executable:
                    raise Refused(path, "immutable identity collision")
            os.unlink(temporary, dir_fd=fd)
            temporary = None
            os.fsync(fd)
        finally:
            if temporary is not None:
                os.unlink(temporary, dir_fd=fd)
            os.close(fd)

    def append_line(self, path, value):
        fd, name = self._parent(path, create=True)
        opened = None
        try:
            opened = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_APPEND | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK, 0o600, dir_fd=fd)
            info = os.fstat(opened)
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_uid != os.getuid() or info.st_mode & 0o077:
                raise Refused(path, "append target must be a private owned regular file")
            data = canonical.record_bytes(value)
            while data:
                written = os.write(opened, data)
                if written == 0:
                    raise OSError("append made no progress")
                data = data[written:]
            os.fsync(opened)
        finally:
            if opened is not None:
                os.close(opened)
            os.close(fd)

    def capture(self, category, identity, files, *, empty_dirs=()):
        """Stage a complete tree, freeze it, then publish under the lab lock."""
        if category not in ("experiments", "subjects") or not digest_value(identity):
            raise Refused("store", "invalid store address")
        expected = entries(files)
        if any(not relative(path) for path in empty_dirs):
            raise Refused("store", "uncontained empty directory")
        parent = self.directory("store/" + category, create=True)
        stage = None
        try:
            try:
                existing = os.open(identity, DIR_FLAGS, dir_fd=parent)
            except FileNotFoundError:
                existing = None
            if existing is not None:
                try:
                    _private(existing, "store")
                    self._verify_frozen(existing)
                    for path in empty_dirs:
                        child = os.dup(existing)
                        try:
                            for component in path.split("/"):
                                opened = os.open(component, DIR_FLAGS, dir_fd=child)
                                os.close(child)
                                child = opened
                        finally:
                            os.close(child)
                    if entries(_snapshot_fd(existing)) != expected:
                        raise Refused(identity, "immutable content store collision")
                finally:
                    os.close(existing)
                return self.root / "store" / category / identity
            # Stage and destination have the same parent/filesystem.
            candidate = ".input-" + secrets.token_hex(16)
            os.mkdir(candidate, 0o700, dir_fd=parent)
            stage = candidate
            stage_fd = os.open(stage, DIR_FLAGS, dir_fd=parent)
            try:
                for path in empty_dirs:
                    target = os.dup(stage_fd)
                    try:
                        for component in path.split("/"):
                            try:
                                os.mkdir(component, 0o700, dir_fd=target)
                            except FileExistsError:
                                pass
                            child = os.open(component, DIR_FLAGS, dir_fd=target)
                            os.close(target)
                            target = child
                    finally:
                        os.close(target)
                for file in files:
                    parts = file["path"].split("/")
                    target = os.dup(stage_fd)
                    try:
                        for component in parts[:-1]:
                            try:
                                os.mkdir(component, 0o700, dir_fd=target)
                            except FileExistsError:
                                pass
                            child = os.open(component, DIR_FLAGS, dir_fd=target)
                            os.close(target)
                            target = child
                        opened = os.open(parts[-1], os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=target)
                        with os.fdopen(opened, "wb") as stream:
                            stream.write(file["data"])
                            stream.flush()
                            os.fchmod(stream.fileno(), 0o500 if file["exec"] else 0o400)
                            os.fsync(stream.fileno())
                    finally:
                        os.close(target)
                self._freeze(stage_fd)
                if entries(_snapshot_fd(stage_fd)) != expected:
                    raise Refused(identity, "staged tree failed verification")
            finally:
                os.close(stage_fd)
            os.rename(stage, identity, src_dir_fd=parent, dst_dir_fd=parent)
            stage = None
            os.fsync(parent)
            return self.root / "store" / category / identity
        finally:
            if stage is not None:
                self._remove_stage(parent, stage)
            os.close(parent)

    def _freeze(self, fd):
        for name in os.listdir(fd):
            info = os.stat(name, dir_fd=fd, follow_symlinks=False)
            if stat.S_ISDIR(info.st_mode):
                child = os.open(name, DIR_FLAGS, dir_fd=fd)
                try:
                    self._freeze(child)
                finally:
                    os.close(child)
        os.fchmod(fd, 0o500)
        os.fsync(fd)

    def _verify_frozen(self, fd):
        _private(fd, "store")
        if stat.S_IMODE(os.fstat(fd).st_mode) != 0o500:
            raise Refused("store", "immutable directory mode must be 0500")
        for name in os.listdir(fd):
            info = os.stat(name, dir_fd=fd, follow_symlinks=False)
            if stat.S_ISDIR(info.st_mode):
                child = os.open(name, DIR_FLAGS, dir_fd=fd)
                try:
                    self._verify_frozen(child)
                finally:
                    os.close(child)
            elif (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1
                  or stat.S_IMODE(info.st_mode) not in (0o400, 0o500)):
                raise Refused(name, "invalid immutable file or mode")

    def _remove_stage(self, parent, name):
        fd = os.open(name, DIR_FLAGS, dir_fd=parent)
        try:
            os.fchmod(fd, 0o700)
            for entry in os.listdir(fd):
                info = os.stat(entry, dir_fd=fd, follow_symlinks=False)
                if stat.S_ISDIR(info.st_mode):
                    self._remove_stage(fd, entry)
                else:
                    os.unlink(entry, dir_fd=fd)
        finally:
            os.close(fd)
        os.rmdir(name, dir_fd=parent)


def open_lab(root):
    return Lab(root)


def git_source(repository, commit, excludes):
    """Export tracked commit blobs; no checkout, hooks, filters or worktree bytes."""
    for path in excludes:
        if not relative(path):
            raise Refused("exclude", "expected a contained relative path")
    environment = {"PATH": "/usr/bin:/bin", "LC_ALL": "C", "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null"}
    base = ["git", "--no-replace-objects", "-C", str(repository)]
    try:
        resolved = subprocess.run(base + ["rev-parse", "--verify", "--end-of-options", commit + "^{commit}"],
                                  env=environment, check=True, capture_output=True).stdout.decode("ascii").strip()
        listing = subprocess.run(base + ["ls-tree", "--full-tree", "-rz", resolved], env=environment,
                                 check=True, capture_output=True).stdout
        files = []
        with subprocess.Popen(base + ["cat-file", "--batch"], env=environment, stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, stderr=subprocess.DEVNULL) as process:
            try:
                for entry in listing.split(b"\0"):
                    if not entry:
                        continue
                    metadata, raw_path = entry.split(b"\t", 1)
                    path = raw_path.decode("utf-8", "strict")
                    if any(path == e or path.startswith(e + "/") for e in excludes):
                        continue
                    mode, kind, oid = metadata.split(b" ")
                    if not relative(path) or mode not in (b"100644", b"100755") or kind != b"blob":
                        raise Refused(path, "source links, submodules and special files are forbidden")
                    process.stdin.write(oid + b"\n")
                    process.stdin.flush()
                    header = process.stdout.readline().split()
                    if len(header) != 3 or header[0] != oid or header[1] != b"blob":
                        raise Refused(path, "git blob unavailable")
                    size = int(header[2])
                    data = process.stdout.read(size)
                    if len(data) != size or process.stdout.read(1) != b"\n":
                        raise Refused(path, "truncated git blob")
                    files.append({"path": path, "data": data, "exec": mode == b"100755"})
            finally:
                process.stdin.close()
            if process.wait() != 0:
                raise Refused("repository", "git could not export source")
        return resolved, files
    except (subprocess.CalledProcessError, UnicodeError) as error:
        raise Refused("repository", "cannot read the commit's UTF-8 tree") from error


def capture_subject(lab, repository, commit, excludes, artifacts):
    resolved, source = git_source(repository, commit, excludes)
    artifact_files = []
    record = {"format": "lab-subject/1", "commit": resolved, "source": canonical.tree_digest(entries(source)),
              "artifacts": {}, "about": {"repository": os.path.abspath(repository), "excluded": sorted(set(excludes))}}
    for name, path in artifacts:
        if not NAME.fullmatch(name) or name in record["artifacts"]:
            raise Refused("artifact", "invalid or duplicate artifact name")
        data, executable = read_regular(path)
        record["artifacts"][name] = {"sha256": file_digest(data), "bytes": len(data), "exec": executable}
        artifact_files.append({"path": "artifacts/" + name, "data": data, "exec": executable})
    refusals = subject_refusals(record)
    if refusals:
        raise Refused(refusals[0].member, refusals[0].why)
    identity = canonical.digest(record)
    files = artifact_files + [{**f, "path": "source/" + f["path"]} for f in source]
    # Provenance is outside identity. An existing valid subject keeps its first provenance.
    try:
        previous = canonical.loads(lab.read(f"store/subjects/{identity}/subject.json"))
    except FileNotFoundError:
        previous = None
    if previous is not None:
        if canonical.digest(previous) != identity:
            raise Refused("subject", "stored subject identity collision")
        record = previous
    files.append({"path": "subject.json", "data": canonical.record_bytes(record), "exec": False})
    lab.capture("subjects", identity, files, empty_dirs=("source",))
    return identity
