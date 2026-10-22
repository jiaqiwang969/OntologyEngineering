"""Portable byte transport and bounded POSIX process supervision.

Project decisions and memory validation remain in the native Rust programs.
This module reads only an explicitly selected checkout's installation record.
"""
import hashlib
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import time


def file_sha(path):
    digest = hashlib.sha256()
    buffer = bytearray(1024 * 1024)
    view = memoryview(buffer)
    with Path(path).open('rb') as stream:
        while count := stream.readinto(buffer):
            digest.update(view[:count])
    return digest.hexdigest()


def root_path(root=None):
    selected = root or os.environ.get('ONTOLOGY_ENGINEERING_ROOT')
    if not selected or not Path(selected).is_absolute():
        raise ValueError('Select an absolute --root or ONTOLOGY_ENGINEERING_ROOT')
    path = Path(selected).resolve(strict=True)
    if not path.is_dir():
        raise ValueError('Selected root must be a directory')
    return path


def project_data_root(root, project_home=None):
    """Select business storage only from an explicit argument, never cwd or env."""
    if project_home is None:
        return root_path(root)
    selected = Path(project_home)
    if not selected.is_absolute() or '..' in selected.parts:
        raise ValueError('Project home must be an explicit absolute path without parent traversal')
    resolved = selected.resolve(strict=True)
    if resolved != selected or not resolved.is_dir():
        raise ValueError('Project home must be an existing real directory without symlinks')
    software = root_path(root)
    if software == resolved or resolved in software.parents:
        raise ValueError('Project home must not contain the software installation')
    private = software / 'var'
    if software in resolved.parents and (resolved == private or private not in resolved.parents):
        raise ValueError('Project home inside the installation must be a private var subdirectory')
    return resolved


def project_file(data_root, selected):
    """Resolve a business file inside one selected store, without root fallback."""
    selected = Path(selected)
    if '..' in selected.parts:
        raise ValueError('Project path contains parent traversal')
    path = selected if selected.is_absolute() else data_root / selected
    if path.resolve() != path:
        raise ValueError('Project path must not contain symlinks')
    path.relative_to(data_root)
    return path


def cli_prefix(program, root, project_home=None):
    args = [str(program), '--root', str(root)]
    if project_home is not None:
        args += ['--project-home', str(project_data_root(root, project_home))]
    return args


def selected_program(root, name):
    root = root_path(root)
    record_path = root / 'var/install/INSTALLATION.json'
    if record_path.resolve(strict=True) != record_path:
        raise ValueError('Installation record must remain inside its real installation path')
    _, record = read_input(record_path)
    if record.get('schema') != 'oev2.installation/1' or record.get('root') != str(root):
        raise ValueError('Installation record differs from selected root')
    matches = [entry for entry in record['programs'] if entry.get('name') == name]
    if len(matches) != 1:
        raise ValueError('Installation must select exactly one ' + name)
    entry = matches[0]
    expected = root / 'var/install/bin' / name
    if entry.get('path') != str(expected) or not expected.is_file() or expected.resolve(strict=True) != expected:
        raise ValueError('Selected native executable is outside this installation')
    if file_sha(expected) != entry.get('sha256'):
        raise ValueError('Selected native executable digest changed')
    return {'path': str(expected), 'sha256': entry['sha256']}

MAX_INPUT_BYTES = 16 * 1024 * 1024


def save(path, value):
    with path.open('x') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write('\n')


def read_bytes(path):
    with path.open('rb') as stream:
        data = stream.read(MAX_INPUT_BYTES + 1)
    if len(data) > MAX_INPUT_BYTES:
        raise ValueError('Input exceeds 16 MiB: ' + str(path))
    return data


def read_input(path):
    data = read_bytes(path)
    return data, json.loads(data)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def signal_group(process, sig):
    # macOS can report EPERM for an exited but unreaped group leader.
    # Reap that owned process and retry; a genuinely live denied group remains
    # visible to the final cleanup check and is never reported as absent.
    process.poll()
    for attempt in range(2):
        try:
            os.killpg(process.pid, sig)
            return True
        except ProcessLookupError:
            return True
        except PermissionError:
            try:
                process.wait(timeout=0.05)
            except subprocess.TimeoutExpired:
                pass
    return False


def group_exists(pid):
    try:
        os.killpg(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def run_process(command, directory, name, deadline, output_limit):
    """Bound raw output and reclaim the owned group from the instant spawn succeeds."""
    if os.name != 'posix':
        raise ValueError('Bounded process-group transport currently requires POSIX')
    if time.monotonic() >= deadline:
        raise TimeoutError('Invocation deadline reached before ' + name)
    save(directory / (name + '.intent.json'), {'command': command})
    with (directory / (name + '.stdout')).open('xb') as out, \
         (directory / (name + '.stderr')).open('xb') as err:
        count, observed, reason = 0, 0, None
        stop_at, selector = None, None
        started = time.monotonic()
        process = subprocess.Popen(command, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, start_new_session=True)
        # Nothing that can fail after Popen belongs outside this cleanup scope.
        try:
            save(directory / (name + '.started.json'), {'pid': process.pid, 'process_group': process.pid})
            selector = selectors.DefaultSelector()
            selector.register(process.stdout, selectors.EVENT_READ, out)
            selector.register(process.stderr, selectors.EVENT_READ, err)
            while selector.get_map():
                now = time.monotonic()
                if reason is None and now >= deadline:
                    reason = 'timeout'
                if reason is not None and stop_at is None:
                    signal_group(process, signal.SIGTERM)
                    stop_at = now
                if process.poll() is not None and stop_at is None:
                    signal_group(process, signal.SIGTERM)
                    stop_at = now
                if stop_at is not None and now - stop_at >= 0.25:
                    signal_group(process, signal.SIGKILL)
                if stop_at is not None and now - stop_at >= 1.0:
                    reason = reason or 'output_pipe_not_closed'
                    break
                for key, _ in selector.select(0.05):
                    chunk = os.read(key.fileobj.fileno(), 65536)
                    if not chunk:
                        selector.unregister(key.fileobj)
                        continue
                    observed += len(chunk)
                    remaining = max(0, output_limit - count)
                    key.data.write(chunk[:remaining])
                    count += min(len(chunk), remaining)
                    if len(chunk) > remaining:
                        reason = reason or 'output_limit'
            if reason is None:
                try:
                    process.wait(timeout=max(0.001, deadline - time.monotonic()))
                except subprocess.TimeoutExpired:
                    reason = 'timeout'
        except KeyboardInterrupt:
            reason = 'interrupted'
        except Exception as error:
            reason = 'supervision_error: ' + str(error)
        finally:
            # Also runs for started-receipt and selector construction/registration failures.
            for sig, timeout in [(signal.SIGTERM, 0.25), (signal.SIGKILL, 1.0)]:
                try:
                    signal_group(process, sig)
                    process.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    if sig == signal.SIGKILL:
                        reason = reason or 'owned_parent_not_reaped'
                except OSError as error:
                    reason = reason or 'cleanup_error: ' + str(error)
            try:
                signal_group(process, signal.SIGKILL)
            except OSError as error:
                reason = reason or 'cleanup_error: ' + str(error)
            for resource in [selector, process.stdout, process.stderr]:
                if resource is not None:
                    try:
                        resource.close()
                    except OSError as error:
                        reason = reason or 'cleanup_error: ' + str(error)
        cleanup_deadline = time.monotonic() + 0.5
        while group_exists(process.pid) and time.monotonic() < cleanup_deadline:
            time.sleep(0.02)
        result = {'command': command, 'pid': process.pid,
                  'exit_code': process.returncode, 'failure': reason,
                  'output_bytes_saved': count, 'output_bytes_observed': observed,
                  'output_complete': reason is None,
                  'process_group_absent': not group_exists(process.pid),
                  'elapsed_seconds': time.monotonic() - started}
        save(directory / (name + '.process.json'), result)
        return result
