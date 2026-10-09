#!/usr/bin/env python3
"""
Prepend IBR boot header to a raw N3290x nandloader binary.
Values taken from the existing loader on this specific board.

Usage:
  python prepend_header.py nandloader.bin -o nandloader_with_header.bin
  python prepend_header.py nandloader.bin -o nandloader_with_header.bin --exec-addr 0x100000
"""

import argparse
import struct
import os


def make_header(image_size: int, exec_addr: int = 0x00100000) -> bytes:
    """
    Build the 32-byte IBR_BOOT_STRUCT_T header.

    Fields (from your chip's dump):
      BootCodeMarker  = 0x57425AA5
      ExeAddr         = 0x00100000  (or user-specified)
      ImageSize       = len(binary)
      SkewMarker      = 0xA55A4257
      DQSODS          = 0x00001010
      CLKQSDS         = 0x00888800
      DRAMMarker      = 0x00000000  (use IBR defaults for stacked SDRAM)
      DRAMSize        = 0x00000000
    """
    return struct.pack('<IIIIIIII',
        0x57425AA5,     # BootCodeMarker
        exec_addr,      # ExeAddr
        image_size,     # ImageSize
        0xA55A4257,     # SkewMarker
        0x00001010,     # DQSODS
        0x00888800,     # CLKQSDS
        0x00000000,     # DRAMMarker
        0x00000000,     # DRAMSize
    )


def main():
    parser = argparse.ArgumentParser(
        description='Prepend IBR boot header to N3290x nandloader binary')
    parser.add_argument('input', help='Raw nandloader binary (no header)')
    parser.add_argument('-o', '--output', required=True,
                        help='Output file with header prepended')
    parser.add_argument('--exec-addr', type=lambda x: int(x, 0),
                        default=0x00100000,
                        help='Execution address (default: 0x00100000)')
    args = parser.parse_args()

    with open(args.input, 'rb') as f:
        binary = f.read()

    # Check if header already exists
    if binary[:4] == b'\xA5\x5A\x42\x57':
        print("WARNING: Binary already starts with BootCodeMarker!")
        print("         It may already have the header. Proceeding anyway.")
        print()

    header = make_header(len(binary), args.exec_addr)

    with open(args.output, 'wb') as f:
        f.write(header)
        f.write(binary)

    total = len(header) + len(binary)
    print(f"Input:      {args.input} ({len(binary)} bytes)")
    print(f"ExeAddr:    0x{args.exec_addr:08X}")
    print(f"ImageSize:  0x{len(binary):X} ({len(binary)} bytes)")
    print(f"Header:     32 bytes prepended")
    print(f"Output:     {args.output} ({total} bytes)")
    print()

    # Sanity checks
    pages_needed = (total + 2047) // 2048
    blocks_needed = (total + 131071) // 131072
    print(f"Will occupy {pages_needed} NAND pages ({blocks_needed} block(s))")
    if total > 131072:
        print("WARNING: Image exceeds one 128KB block!")
        print("         Each of the 4 redundant copies needs multiple blocks.")


if __name__ == '__main__':
    main()
