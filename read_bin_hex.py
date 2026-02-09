#!/usr/bin/env python3
"""
Read interface for blackhole_0.bin with spi_read-like interface.
Reads from the binary file at a specified address and size, then outputs
a human-readable hex dump with addresses in a column on the left.
"""

import argparse
import sys
from pathlib import Path


def read_bin_hex(bin_file: Path, addr: int, size: int, output_file: Path = None, bytes_per_line: int = 16):
    """
    Read from binary file at specified address and size, output hex dump.
    
    Args:
        bin_file: Path to input .bin file
        addr: Starting address (offset in bytes)
        size: Number of bytes to read
        output_file: Path to output file (None for stdout)
        bytes_per_line: Number of bytes to display per line
    """
    try:
        with open(bin_file, 'rb') as f:
            # Check file size
            f.seek(0, 2)  # Seek to end
            file_size = f.tell()
            
            # Check if address is valid
            if addr >= file_size:
                print(f"Error: Address 0x{addr:x} ({addr}) exceeds file size {file_size} bytes", file=sys.stderr)
                sys.exit(1)
            
            # Adjust size if it would exceed file bounds
            max_read = file_size - addr
            if size > max_read:
                print(f"Warning: Requested size {size} exceeds available bytes ({max_read}). Reading {max_read} bytes.", file=sys.stderr)
                size = max_read
            
            # Read the data
            f.seek(addr)
            data = f.read(size)
            
    except FileNotFoundError:
        print(f"Error: File '{bin_file}' not found.", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"Error reading file: {e}", file=sys.stderr)
        sys.exit(1)
    
    if len(data) == 0:
        print("Warning: No data read.", file=sys.stderr)
        return
    
    output = sys.stdout if output_file is None else open(output_file, 'w')
    
    try:
        # Print header
        output.write(f"Reading from {bin_file} at address 0x{addr:x} ({addr}), size {len(data)} bytes\n")
        output.write("=" * 80 + "\n")
        output.write(f"{'Address':<12} {'Hex Data':<48} {'ASCII':<16}\n")
        output.write("-" * 80 + "\n")
        
        # Print hex dump
        for i in range(0, len(data), bytes_per_line):
            chunk = data[i:i + bytes_per_line]
            current_addr = addr + i
            
            # Format address in left column
            output.write(f"0x{current_addr:08x}  ")
            
            # Format hex bytes
            hex_bytes = ' '.join(f"{b:02x}" for b in chunk)
            output.write(hex_bytes)
            
            # Pad to align ASCII column
            padding = ' ' * (3 * (bytes_per_line - len(chunk)))
            output.write(padding)
            
            # Add ASCII representation
            ascii_repr = ''.join(chr(b) if 32 <= b < 127 else '.' for b in chunk)
            output.write(f"  |{ascii_repr}|\n")
        
        output.write("=" * 80 + "\n")
        
        if output_file is not None:
            output.close()
            print(f"Hex dump written to '{output_file}'", file=sys.stderr)
    except Exception as e:
        print(f"Error writing output: {e}", file=sys.stderr)
        sys.exit(1)


def main():
    parser = argparse.ArgumentParser(
        description='Read from blackhole_0.bin with spi_read-like interface and output hex dump',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s 0x0 256                    # Read 256 bytes from address 0
  %(prog)s 0x1000 512                 # Read 512 bytes from address 0x1000
  %(prog)s 0x0 256 -o output.hex      # Save to file
  %(prog)s 0x0 256 -n 32              # 32 bytes per line
  %(prog)s --file other.bin 0x0 256   # Read from different file
        """
    )
    parser.add_argument('addr', type=lambda x: int(x, 0),  # Auto-detect hex (0x) or decimal
                       help='Starting address (hex: 0x1000 or decimal: 4096)')
    parser.add_argument('size', type=int,
                       help='Number of bytes to read')
    parser.add_argument('-f', '--file', type=Path, default=Path('flash_dump/blackhole_1.bin'),
                       help='Binary file to read from (default: flash_dump/blackhole_1.bin)')
    parser.add_argument('-o', '--output', type=Path, default=None,
                       help='Output file (default: stdout)')
    parser.add_argument('-n', '--bytes-per-line', type=int, default=16,
                       help='Number of bytes per line (default: 16)')
    
    args = parser.parse_args()
    
    if args.addr < 0:
        print("Error: Address must be non-negative.", file=sys.stderr)
        sys.exit(1)
    
    if args.size <= 0:
        print("Error: Size must be positive.", file=sys.stderr)
        sys.exit(1)
    
    if not args.file.exists():
        print(f"Error: Input file '{args.file}' does not exist.", file=sys.stderr)
        sys.exit(1)
    
    read_bin_hex(
        args.file,
        args.addr,
        args.size,
        args.output,
        args.bytes_per_line
    )


if __name__ == '__main__':
    main()

