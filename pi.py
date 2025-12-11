#!/usr/bin/env python3
"""Print N digits of pi."""

import argparse
from mpmath import mp


def main():
    parser = argparse.ArgumentParser(description="Print N digits of pi")
    parser.add_argument("n", type=int, help="Number of digits to print")
    args = parser.parse_args()

    if args.n < 1:
        parser.error("N must be at least 1")

    mp.dps = args.n + 1  # Extra precision to avoid rounding issues
    print(str(mp.pi)[:args.n + 1])  # +1 for the "3."


if __name__ == "__main__":
    main()
