#!/bin/bash

set -euo pipefail

if [[ -z "${CODE_FD:-}" || ! -f "${CODE_FD:-/nonexistent}" ]]; then
  for p in \
    /opt/homebrew/share/qemu/edk2-x86_64-code.fd \
    /usr/local/share/qemu/edk2-x86_64-code.fd \
    /opt/homebrew/Cellar/qemu/*/share/qemu/edk2-x86_64-code.fd \
    /usr/local/Cellar/qemu/*/share/qemu/edk2-x86_64-code.fd; do
    if [[ -f "$p" ]]; then CODE_FD="$p"; break; fi
  done
fi
if [[ -z "${VARS_FD_TEMPLATE:-}" || ! -f "${VARS_FD_TEMPLATE:-/nonexistent}" ]]; then
  for p in \
    /opt/homebrew/share/qemu/edk2-x86_64-vars.fd \
    /usr/local/share/qemu/edk2-x86_64-vars.fd \
    /opt/homebrew/Cellar/qemu/*/share/qemu/edk2-x86_64-vars.fd \
    /usr/local/Cellar/qemu/*/share/qemu/edk2-x86_64-vars.fd \
    /opt/homebrew/share/qemu/edk2-i386-vars.fd \
    /usr/local/share/qemu/edk2-i386-vars.fd \
    /opt/homebrew/Cellar/qemu/*/share/qemu/edk2-i386-vars.fd \
    /usr/local/Cellar/qemu/*/share/qemu/edk2-i386-vars.fd; do
    if [[ -f "$p" ]]; then VARS_FD_TEMPLATE="$p"; break; fi
  done
fi

if [[ -z "${VARS_FD_TEMPLATE:-}" || ! -f "$VARS_FD_TEMPLATE" ]]; then
  for j in /opt/homebrew/share/qemu/firmware/60-edk2-x86_64.json /usr/local/share/qemu/firmware/60-edk2-x86_64.json; do
    if [[ -f "$j" ]]; then
      cand=$(sed -n 's/.*"nvram-template".*"filename": "\([^"]\+\)".*/\1/p' "$j")
      if [[ -n "$cand" && -f "$cand" ]]; then VARS_FD_TEMPLATE="$cand"; break; fi
    fi
  done
fi
if [[ -z "${CODE_FD:-}" || ! -f "$CODE_FD" ]]; then
  echo "Could not locate edk2-x86_64-code.fd. Set CODE_FD to its path." >&2
  exit 1
fi
if [[ -z "${VARS_FD_TEMPLATE:-}" || ! -f "$VARS_FD_TEMPLATE" ]]; then
  echo "Warning: Could not locate UEFI vars template; proceeding without persistent NVRAM." >&2
fi

cargo build -p zap --target x86_64-unknown-uefi

rm -rf target/esp
mkdir -p target/esp/EFI/BOOT

echo "Hello from the filesystem!" > target/esp/hello.txt

cp target/x86_64-unknown-uefi/debug/zap.efi target/esp/EFI/BOOT/BOOTX64.EFI

printf '\\EFI\\BOOT\\BOOTX64.EFI\r\n' > target/esp/startup.nsh

qemu-img create -f raw target/esp.img 100M
mformat -i target/esp.img -F ::
mcopy -i target/esp.img -s target/esp/* ::

if [[ -n "${VARS_FD_TEMPLATE:-}" && -f "$VARS_FD_TEMPLATE" ]]; then
  cp "$VARS_FD_TEMPLATE" target/edk2_vars.fd
fi

qemu-system-x86_64 \
    -machine q35 \
    -m 128M \
    -device qemu-xhci,id=xhci \
    -device usb-kbd,bus=xhci.0 \
    -device usb-mouse,bus=xhci.0 \
    -drive if=pflash,format=raw,readonly=on,file="$CODE_FD" \
    ${VARS_FD_TEMPLATE:+-drive if=pflash,format=raw,file=target/edk2_vars.fd} \
    -drive if=none,id=esp,format=raw,file=target/esp.img \
    -device usb-storage,drive=esp,bus=xhci.0
