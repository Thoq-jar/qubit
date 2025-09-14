#!/usr/bin/env python3

import os
import sys
import subprocess
import shutil
import json
import re
from pathlib import Path


def find_file_in_paths(paths):
    for path_pattern in paths:
        if "*" in path_pattern:
            parent = Path(path_pattern).parent
            pattern = Path(path_pattern).name
            if parent.exists():
                for match in parent.glob(pattern):
                    if match.is_file():
                        return str(match)
        else:
            if Path(path_pattern).is_file():
                return path_pattern
    return None


def main():
    code_fd = os.environ.get("CODE_FD")
    if not code_fd or not Path(code_fd).is_file():
        code_fd_paths = [
            "/opt/homebrew/share/qemu/edk2-x86_64-code.fd",
            "/usr/local/share/qemu/edk2-x86_64-code.fd",
            "/opt/homebrew/Cellar/qemu/*/share/qemu/edk2-x86_64-code.fd",
            "/usr/local/Cellar/qemu/*/share/qemu/edk2-x86_64-code.fd",
        ]
        code_fd = find_file_in_paths(code_fd_paths)

    vars_fd_template = os.environ.get("VARS_FD_TEMPLATE")
    if not vars_fd_template or not Path(vars_fd_template).is_file():
        vars_fd_paths = [
            "/opt/homebrew/share/qemu/edk2-x86_64-vars.fd",
            "/usr/local/share/qemu/edk2-x86_64-vars.fd",
            "/opt/homebrew/Cellar/qemu/*/share/qemu/edk2-x86_64-vars.fd",
            "/usr/local/Cellar/qemu/*/share/qemu/edk2-x86_64-vars.fd",
            "/opt/homebrew/share/qemu/edk2-i386-vars.fd",
            "/usr/local/share/qemu/edk2-i386-vars.fd",
            "/opt/homebrew/Cellar/qemu/*/share/qemu/edk2-i386-vars.fd",
            "/usr/local/Cellar/qemu/*/share/qemu/edk2-i386-vars.fd",
        ]
        vars_fd_template = find_file_in_paths(vars_fd_paths)

    if not vars_fd_template or not Path(vars_fd_template).is_file():
        json_paths = [
            "/opt/homebrew/share/qemu/firmware/60-edk2-x86_64.json",
            "/usr/local/share/qemu/firmware/60-edk2-x86_64.json",
        ]

        for json_path in json_paths:
            if Path(json_path).is_file():
                try:
                    with open(json_path, "r") as f:
                        content = f.read()
                        match = re.search(
                            r'"nvram-template".*"filename":\s*"([^"]+)"', content
                        )
                        if match:
                            candidate = match.group(1)
                            if Path(candidate).is_file():
                                vars_fd_template = candidate
                                break
                except Exception:
                    continue

    if not code_fd or not Path(code_fd).is_file():
        print(
            "Could not locate edk2-x86_64-code.fd. Set CODE_FD to its path.",
            file=sys.stderr,
        )
        sys.exit(1)

    if not vars_fd_template or not Path(vars_fd_template).is_file():
        print(
            "Warning: Could not locate UEFI vars template; proceeding without persistent NVRAM.",
            file=sys.stderr,
        )
        vars_fd_template = None

    try:
        subprocess.run(
            ["cargo", "build", "-p", "zap", "--target", "x86_64-unknown-uefi"],
            check=True,
        )

        esp_dir = Path("target/esp")
        if esp_dir.exists():
            shutil.rmtree(esp_dir)

        esp_boot_dir = esp_dir / "EFI" / "BOOT"
        esp_boot_dir.mkdir(parents=True)

        with open(esp_dir / "hello.txt", "w") as f:
            f.write("Hello from the filesystem!")

        src_efi = Path("target/x86_64-unknown-uefi/debug/zap.efi")
        dst_efi = esp_boot_dir / "BOOTX64.EFI"
        shutil.copy2(src_efi, dst_efi)

        with open(esp_dir / "startup.nsh", "wb") as f:
            f.write(b"\\EFI\\BOOT\\BOOTX64.EFI\r\n")

        subprocess.run(
            ["qemu-img", "create", "-f", "raw", "target/esp.img", "100M"], check=True
        )
        subprocess.run(["mformat", "-i", "target/esp.img", "-F", "::"], check=True)
        for item in esp_dir.iterdir():
            subprocess.run(
                ["mcopy", "-i", "target/esp.img", "-s", str(item), "::"], check=True
            )

        if vars_fd_template:
            shutil.copy2(vars_fd_template, "target/edk2_vars.fd")

        qemu_cmd = [
            "qemu-system-x86_64",
            "-machine",
            "q35",
            "-m",
            "128M",
            "-device",
            "qemu-xhci,id=xhci",
            "-device",
            "usb-kbd,bus=xhci.0",
            "-device",
            "usb-mouse,bus=xhci.0",
            "-drive",
            f"if=pflash,format=raw,readonly=on,file={code_fd}",
        ]

        if vars_fd_template:
            qemu_cmd.extend(["-drive", "if=pflash,format=raw,file=target/edk2_vars.fd"])

        qemu_cmd.extend(
            [
                "-drive",
                "if=none,id=esp,format=raw,file=target/esp.img",
                "-device",
                "usb-storage,drive=esp,bus=xhci.0",
            ]
        )

        subprocess.run(qemu_cmd, check=True)

    except subprocess.CalledProcessError as e:
        print(f"Command failed: {e}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
