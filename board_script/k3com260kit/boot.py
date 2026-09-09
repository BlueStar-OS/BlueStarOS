#!/usr/bin/env python3
"""Build, stage, and boot BlueStarOS on the SpacemiT K3 COM260 Kit.

``kernel/Makefile`` builds a U-Boot FIT image with ``make itb``. The FIT image
contains both the BlueStarOS kernel and its DTB, so the board only needs one
fastboot download followed by ``bootm``.
"""

from __future__ import annotations

import argparse
import os
import re
import select
import shlex
import shutil
import subprocess
import sys
import termios
import threading
import time
import tty
from pathlib import Path


PROJECT_ROOT = Path(__file__).resolve().parents[2]

# Keep the artifact path in sync with kernel/Makefile's ITB_BUILD_DIR/ITB_OUT.
DEFAULT_ITB = Path("tmp/build/itb/bluestaros.itb")
DEFAULT_K3_DTB = Path(
    "kernel/src/dtb/spacemit_k3_com260kit/spacemit-k3-com260-ifx.dtb"
)

# Temporary address at which K3 U-Boot receives the FIT image. The linker
# address is selected by the Cargo board feature and read from the final ELF.
# Keeping these addresses separate prevents bootm from parsing an image while
# overwriting it.
DEFAULT_ITB_LOAD_ADDR = "0x180000000"
FASTBOOT_SIZE_ALIGNMENT = 0x100000


def status(message: str) -> None:
    print(f"[k3com260kit] {message}", file=sys.stderr, flush=True)


def resolve_project_path(value: Path) -> Path:
    path = value.expanduser()
    if not path.is_absolute():
        path = PROJECT_ROOT / path
    return path.resolve()


def default_dtb() -> Path | None:
    """Select the K3 DTB, unless the caller explicitly overrides it.

    The board profile owns the default DTB in ``kernel/Makefile``. This
    convenience lookup keeps the boot script's dry-run command explicit.
    """
    configured = os.environ.get("DTB_FILE")
    if configured:
        return Path(configured)

    configured_dir = os.environ.get("DTB_DIR")
    dtb_dir = resolve_project_path(Path(configured_dir or "config/dtb"))
    candidates = sorted(dtb_dir.glob("*.dtb"))
    if not candidates:
        board_dtb = PROJECT_ROOT / DEFAULT_K3_DTB
        return DEFAULT_K3_DTB if board_dtb.is_file() else None
    try:
        return candidates[0].relative_to(PROJECT_ROOT)
    except ValueError:
        return candidates[0]


def make_command(args: argparse.Namespace, image: Path, dtb: Path | None) -> list[str]:
    """Build the kernel Makefile target that produces the FIT image."""
    command = [
        args.make,
        "-C",
        str(PROJECT_ROOT / "kernel"),
        "itb",
        "BOARD=spacemitk3-com260kit",
        f"ITB_OUT={image}",
    ]
    if dtb is not None:
        command.append(f"DTB_FILE={dtb}")
    if args.build_user_image:
        command.append("SKIP_USER_IMAGE=0")
    command.extend(args.make_variable)
    return command


def build_image(args: argparse.Namespace, image: Path, dtb: Path | None) -> None:
    """Invoke ``kernel/Makefile``'s ``itb`` target."""
    command = make_command(args, image, dtb)
    status("Build: " + shlex.join(command))
    subprocess.run(command, cwd=PROJECT_ROOT, check=True)


def fastboot_size(artifact: Path, configured_size: str | None) -> str:
    """Return a fastboot receive limit large enough for the FIT image."""
    if configured_size:
        return configured_size

    size = artifact.stat().st_size
    aligned = (
        (size + FASTBOOT_SIZE_ALIGNMENT - 1) // FASTBOOT_SIZE_ALIGNMENT
    ) * FASTBOOT_SIZE_ALIGNMENT
    return f"0x{max(aligned, FASTBOOT_SIZE_ALIGNMENT):x}"


class SerialConsole:
    def __init__(self, device: Path, baud: int, prompt_regex: bytes) -> None:
        self.device = device
        self.prompt_regex = re.compile(prompt_regex, re.MULTILINE)
        self._fd = os.open(device, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
        self._output = bytearray()
        self._condition = threading.Condition()
        self._stop = threading.Event()
        self._configure(baud)
        self._reader = threading.Thread(
            target=self._read_serial,
            name="k3-serial-reader",
            daemon=True,
        )
        self._reader.start()

    def _configure(self, baud: int) -> None:
        speed = getattr(termios, f"B{baud}", None)
        if speed is None:
            raise ValueError(f"unsupported baud rate: {baud}")

        attrs = termios.tcgetattr(self._fd)
        attrs[0] = 0
        attrs[1] = 0
        attrs[2] &= ~(termios.CSIZE | termios.PARENB | termios.CSTOPB)
        attrs[2] |= termios.CS8 | termios.CLOCAL | termios.CREAD
        if hasattr(termios, "CRTSCTS"):
            attrs[2] &= ~termios.CRTSCTS
        attrs[3] = 0
        attrs[4] = speed
        attrs[5] = speed
        attrs[6][termios.VMIN] = 0
        attrs[6][termios.VTIME] = 1
        termios.tcsetattr(self._fd, termios.TCSANOW, attrs)
        termios.tcflush(self._fd, termios.TCIOFLUSH)

    def _read_serial(self) -> None:
        while not self._stop.is_set():
            try:
                ready, _, _ = select.select([self._fd], [], [], 0.1)
                if not ready:
                    continue
                data = os.read(self._fd, 4096)
                if not data:
                    continue
            except OSError:
                if not self._stop.is_set():
                    status("serial reader stopped unexpectedly")
                return

            try:
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()
            except BrokenPipeError:
                pass

            with self._condition:
                self._output.extend(data)
                if len(self._output) > 256 * 1024:
                    del self._output[: len(self._output) - 128 * 1024]
                self._condition.notify_all()

    def clear_output(self) -> None:
        with self._condition:
            self._output.clear()

    def write(self, data: bytes) -> None:
        offset = 0
        while offset < len(data):
            try:
                offset += os.write(self._fd, data[offset:])
            except BlockingIOError:
                select.select([], [self._fd], [], 0.1)

    def send_command(self, command: str) -> None:
        status(f"U-Boot: {command}")
        self.clear_output()
        self.write(command.encode("ascii") + b"\r")

    def wait_for_prompt(self, timeout: float) -> None:
        deadline = time.monotonic() + timeout
        with self._condition:
            while not self.prompt_regex.search(bytes(self._output)):
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError(
                        f"U-Boot prompt not seen on {self.device} within {timeout:g}s"
                    )
                self._condition.wait(min(remaining, 0.25))

    def interrupt_to_prompt(self, timeout: float) -> None:
        status("U-Boot: Ctrl-C")
        self.clear_output()
        self.write(b"\x03")
        time.sleep(0.1)
        self.write(b"\r")
        self.wait_for_prompt(timeout)

    def interrupt_autoboot(self, timeout: float) -> None:
        status("U-Boot: repeatedly sending 's' to interrupt autoboot")
        deadline = time.monotonic() + timeout
        self.clear_output()

        while True:
            with self._condition:
                if self.prompt_regex.search(bytes(self._output)):
                    break
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError(
                        f"U-Boot prompt not seen on {self.device} within {timeout:g}s"
                    )

            self.write(b"s")
            with self._condition:
                self._condition.wait(min(remaining, 0.02))

        self.interrupt_to_prompt(max(deadline - time.monotonic(), 1.0))

    def interact(self) -> None:
        if not sys.stdin.isatty():
            status("stdin is not a terminal; serial console handoff skipped")
            return

        status("serial console attached; press Ctrl-] to exit")
        stdin_fd = sys.stdin.fileno()
        saved_attrs = termios.tcgetattr(stdin_fd)
        try:
            tty.setraw(stdin_fd)
            while True:
                ready, _, _ = select.select([stdin_fd], [], [], 0.25)
                if not ready:
                    continue
                data = os.read(stdin_fd, 1024)
                if not data:
                    return
                exit_at = data.find(b"\x1d")
                if exit_at >= 0:
                    if exit_at:
                        self.write(data[:exit_at])
                    return
                self.write(data)
        finally:
            termios.tcsetattr(stdin_fd, termios.TCSANOW, saved_attrs)
            print()

    def close(self) -> None:
        self._stop.set()
        try:
            os.close(self._fd)
        finally:
            self._reader.join(timeout=1)

    def __enter__(self) -> SerialConsole:
        return self

    def __exit__(self, _exc_type, _exc_value, _traceback) -> None:
        self.close()


def run_host_fastboot(
    fastboot: str,
    artifact: Path,
    timeout: float,
    dry_run: bool,
) -> None:
    command = [fastboot, "stage", str(artifact)]
    status("Host: " + " ".join(command))
    if dry_run:
        return
    subprocess.run(command, cwd=PROJECT_ROOT, check=True, timeout=timeout)


def stage_artifact(
    console: SerialConsole,
    uboot_command: str,
    artifact: Path,
    args: argparse.Namespace,
) -> None:
    console.send_command(uboot_command)
    time.sleep(args.usb_settle)
    try:
        run_host_fastboot(args.fastboot, artifact, args.stage_timeout, False)
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
        try:
            console.interrupt_to_prompt(args.prompt_timeout)
        except TimeoutError:
            pass
        raise
    console.interrupt_to_prompt(args.prompt_timeout)


def print_dry_run(
    args: argparse.Namespace,
    image: Path,
    dtb: Path | None,
) -> None:
    commands = [
        ("Host", shlex.join(make_command(args, image, dtb))),
        ("U-Boot", "Repeated 's' until the autoboot is interrupted"),
        (
            "U-Boot",
            f"fastboot -l {args.image_load_addr} -s <aligned FIT size> usb 0",
        ),
        ("Host", f"{args.fastboot} stage {image}"),
        ("U-Boot", "Ctrl-C"),
        ("U-Boot", f"bootm {args.image_load_addr}"),
    ]
    for side, command in commands:
        print(f"{side}: {command}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build and boot a BlueStarOS FIT image through K3 U-Boot fastboot."
    )
    parser.add_argument(
        "--serial",
        type=Path,
        default=Path(os.environ.get("K3_SERIAL", "/dev/ttyUSB0")),
        help="U-Boot serial device (default: %(default)s or K3_SERIAL)",
    )
    parser.add_argument(
        "--baud",
        type=int,
        default=int(os.environ.get("K3_BAUD", "115200")),
        help="serial baud rate (default: %(default)s or K3_BAUD)",
    )
    parser.add_argument(
        "--image",
        type=Path,
        default=Path(os.environ.get("BLUESTAROS_ITB", str(DEFAULT_ITB))),
        help="FIT image from kernel/Makefile (default: %(default)s or BLUESTAROS_ITB)",
    )
    parser.add_argument(
        "--dtb",
        type=Path,
        default=default_dtb(),
        help="board DTB passed as DTB_FILE (default: DTB_FILE or config/dtb/*.dtb)",
    )
    parser.add_argument(
        "--make",
        default=os.environ.get("MAKE", "make"),
        help="GNU Make executable (default: %(default)s or MAKE)",
    )
    parser.add_argument(
        "--make-variable",
        action="append",
        default=[],
        metavar="NAME=VALUE",
        help="extra variable passed to kernel/Makefile; may be repeated",
    )
    parser.add_argument(
        "--no-build",
        action="store_true",
        help="use an existing FIT image instead of running make itb",
    )
    parser.add_argument(
        "--build-user-image",
        action="store_true",
        help="also rebuild the privileged disk image before building the kernel",
    )
    parser.add_argument(
        "--image-load-addr",
        default=os.environ.get("K3_IMAGE_LOAD_ADDR", DEFAULT_ITB_LOAD_ADDR),
        help="temporary U-Boot FIT address (default: %(default)s or K3_IMAGE_LOAD_ADDR)",
    )
    parser.add_argument(
        "--image-load-size",
        default=os.environ.get("K3_IMAGE_LOAD_SIZE"),
        help="fastboot receive limit; defaults to FIT size rounded to 1 MiB",
    )
    parser.add_argument(
        "--fastboot",
        default=os.environ.get("FASTBOOT", "fastboot"),
        help="Host fastboot executable (default: %(default)s or FASTBOOT)",
    )
    parser.add_argument("--prompt-regex", default=r"(?m)^=>\s*")
    parser.add_argument("--prompt-timeout", type=float, default=10.0)
    parser.add_argument("--stage-timeout", type=float, default=120.0)
    parser.add_argument("--usb-settle", type=float, default=1.0)
    parser.add_argument(
        "--no-console",
        action="store_true",
        help="exit after bootm instead of attaching stdin to the serial console",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the Host/U-Boot sequence without opening the serial device",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    image = resolve_project_path(args.image)
    dtb = resolve_project_path(args.dtb) if args.dtb is not None else None

    if args.dry_run:
        print_dry_run(args, image, dtb)
        return 0

    if not args.no_build:
        try:
            build_image(args, image, dtb)
        except (OSError, subprocess.SubprocessError) as error:
            status(f"build failed: {error}")
            return 2

    if not image.is_file():
        status(f"missing FIT image: {image}")
        return 2

    if shutil.which(args.fastboot) is None:
        status(f"Host fastboot executable not found: {args.fastboot}")
        return 2

    image_fastboot = (
        f"fastboot -l {args.image_load_addr} "
        f"-s {fastboot_size(image, args.image_load_size)} usb 0"
    )

    try:
        with SerialConsole(
            args.serial,
            args.baud,
            args.prompt_regex.encode("utf-8"),
        ) as console:
            console.interrupt_autoboot(args.prompt_timeout)
            stage_artifact(console, image_fastboot, image, args)
            console.send_command(f"bootm {args.image_load_addr}")
            if args.no_console:
                time.sleep(0.5)
            else:
                console.interact()
    except (OSError, ValueError, TimeoutError, subprocess.SubprocessError) as error:
        status(f"failed: {error}")
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
