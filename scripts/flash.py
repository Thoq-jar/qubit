#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import shlex
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


def run(cmd: list[str] | str, check: bool = True) -> subprocess.CompletedProcess:
    if isinstance(cmd, str):
        shell = True
        printable = cmd
    else:
        shell = False
        printable = " ".join(shlex.quote(c) for c in cmd)
    try:
        return subprocess.run(
            cmd,
            shell=shell,
            check=check,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
    except subprocess.CalledProcessError as e:
        print(f"Command failed: {printable}", file=sys.stderr)
        print(e.stdout, file=sys.stderr)
        print(e.stderr, file=sys.stderr)
        raise


@dataclass
class Disk:
    path: str
    display_path: str
    size: str
    model: str
    transport: str
    removable: bool
    internal: bool


def list_disks_macos() -> list[Disk]:
    out = run(["diskutil", "list"]).stdout
    disks: list[Disk] = []
    header_re = re.compile(r"^/dev/(disk\d+) \(([^)]*)\):\s*$")
    size_re = re.compile(r"^\s*#: +TYPE +NAME +SIZE .*?\n")
    current = None
    for line in out.splitlines():
        m = header_re.match(line.strip())
        if m:
            dev = m.group(1)
            flags = m.group(2)
            internal = "internal" in flags
            external = "external" in flags
            info = run(["diskutil", "info", f"/dev/{dev}"], check=False).stdout
            size_match = re.search(r"Disk Size:\s*([0-9.]+\s*[A-Z]+)", info)
            size = size_match.group(1) if size_match else "?"
            model_match = re.search(r"Device / Media Name:\s*(.*)", info)
            model = (model_match.group(1).strip() if model_match else "").strip()
            transport_match = re.search(r"Protocol:\s*(.*)", info)
            transport = (
                transport_match.group(1).strip() if transport_match else ""
            ).strip()
            disks.append(
                Disk(
                    path=f"/dev/{dev}",
                    display_path=f"/dev/r{dev}",
                    size=size,
                    model=model or dev,
                    transport=transport or ("USB" if external else ""),
                    removable=external,
                    internal=internal,
                )
            )
    return disks


def list_disks_linux() -> list[Disk]:
    if not shutil.which("lsblk"):
        print("lsblk not found. Please install util-linux.", file=sys.stderr)
        return []
    res = run(["lsblk", "-J", "-o", "NAME,KNAME,TYPE,RM,SIZE,MODEL,TRAN,VENDOR"]).stdout
    data = json.loads(res)
    disks: list[Disk] = []
    for dev in data.get("blockdevices", []):
        if dev.get("type") != "disk":
            continue
        name = dev.get("kname") or dev.get("name")
        path = f"/dev/{name}"
        size = dev.get("size") or "?"
        model = " ".join(filter(None, [dev.get("vendor"), dev.get("model")])).strip()
        transport = dev.get("tran") or ""
        removable = bool(dev.get("rm")) or transport.lower() in {"usb"}
        disks.append(
            Disk(
                path=path,
                display_path=path,
                size=size,
                model=model or name,
                transport=transport,
                removable=removable,
                internal=not removable,
            )
        )
    return disks


def ensure_image(path: Path) -> None:
    print("Building UEFI image...")
    run(
        ["cargo", "build", "-p", "zap", "--target", "x86_64-unknown-uefi"],
    )

    esp_dir = Path("target/esp")
    if esp_dir.exists():
        shutil.rmtree(esp_dir)

    esp_boot_dir = esp_dir / "EFI" / "BOOT"
    esp_boot_dir.mkdir(parents=True)

    src_efi = Path("target/x86_64-unknown-uefi/debug/zap.efi")
    dst_efi = esp_boot_dir / "BOOTX64.EFI"
    shutil.copy2(src_efi, dst_efi)

    with open(esp_dir / "startup.nsh", "wb") as f:
        f.write(br"\EFI\BOOT\BOOTX64.EFI\r\n")

    run(["qemu-img", "create", "-f", "raw", "target/esp.img", "100M"])
    run(["mformat", "-i", "target/esp.img", "-F", "::"])
    for item in esp_dir.iterdir():
        run(["mcopy", "-i", "target/esp.img", "-s", str(item), "::"])

    if not path.exists():
        print(f"Image not found: {path}", file=sys.stderr)
        print("Build failed, esp.img not created.", file=sys.stderr)
        sys.exit(1)



def unmount_all_linux(disk: Disk) -> None:
    res = run(["lsblk", "-J", disk.path]).stdout
    data = json.loads(res)
    for dev in data.get("blockdevices", []):
        for ch in dev.get("children", []) or []:
            mp = ch.get("mountpoint")
            if mp:
                print(f"- Unmount {ch.get('name')} from {mp}")
                run(["sudo", "umount", mp], check=False)


def flash_macos(image: Path, disk: Disk) -> None:
    print(f"Using macOS disk: {disk.path} ({disk.size}, {disk.model})")
    run(["diskutil", "unmountDisk", disk.path])
    of = disk.display_path
    dd_cmd = ["sudo", "dd", f"if={image}", f"of={of}", "bs=1m", "conv=sync"]
    print("- Running:", " ".join(shlex.quote(c) for c in dd_cmd))
    run(dd_cmd)
    run(["sync"], check=False)
    run(["diskutil", "eject", disk.path], check=False)
    print("Done. You can now remove the USB drive.")


def flash_linux(image: Path, disk: Disk) -> None:
    print(f"Using Linux disk: {disk.path} ({disk.size}, {disk.model})")
    unmount_all_linux(disk)
    dd_cmd = ["sudo", "dd", f"if={image}", f"of={disk.path}", "bs=4M", "conv=fsync"]
    print("- Running:", " ".join(shlex.quote(c) for c in dd_cmd))
    run(dd_cmd)
    run(["sync"], check=False)
    if shutil.which("udisksctl"):
        run(["udisksctl", "power-off", "-b", disk.path], check=False)
    print("Done. You can now remove the USB drive.")


def choose_disk(disks: list[Disk], include_internal: bool) -> Disk:
    candidates = [d for d in disks if d.removable or include_internal]
    if not candidates:
        print("No suitable disks found. Try --include-internal to show all.")
        sys.exit(2)
    print("Available disks:")
    for idx, d in enumerate(candidates):
        tag = "USB" if d.removable else ("INTERNAL" if d.internal else "")
        print(f"  [{idx}] {d.path:12} {d.size:>8}  {d.model}  {tag}")
    while True:
        sel = input("Select disk index to flash: ").strip()
        if not sel.isdigit():
            print("Enter a valid index.")
            continue
        i = int(sel)
        if 0 <= i < len(candidates):
            return candidates[i]
        print("Index out of range.")


def confirm_destruction(disk: Disk, auto_yes: bool) -> None:
    print("\nWARNING: This will ERASE all data on:")
    print(f"  {disk.path}  ({disk.size}, {disk.model})\n")
    if auto_yes:
        return
    phrase = f"ERASE {disk.path}"
    resp = input(f"Type '{phrase}' to continue: ").strip()
    if resp != phrase:
        print("Aborted.")
        sys.exit(3)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Flash target/esp.img to a USB drive (DANGEROUS)."
    )
    parser.add_argument(
        "--image",
        default="target/esp.img",
        help="Path to ESP image (default: target/esp.img)",
    )
    parser.add_argument("--yes", action="store_true", help="Skip confirmation prompt")
    parser.add_argument(
        "--include-internal",
        action="store_true",
        help="Include internal disks in selection (dangerous)",
    )
    args = parser.parse_args()

    image = Path(args.image)
    ensure_image(image)

    sysname = platform.system().lower()
    if sysname == "darwin":
        disks = list_disks_macos()
        if not disks:
            print("No disks found via diskutil.")
            sys.exit(2)
        disk = choose_disk(disks, include_internal=args.include_internal)
        confirm_destruction(disk, args.yes)
        flash_macos(image, disk)
    elif sysname == "linux":
        disks = list_disks_linux()
        if not disks:
            print("No disks found via lsblk.")
            sys.exit(2)
        disk = choose_disk(disks, include_internal=args.include_internal)
        confirm_destruction(disk, args.yes)
        flash_linux(image, disk)
    else:
        print(f"Unsupported OS: {platform.system()}")
        sys.exit(2)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nAborted by user.")
