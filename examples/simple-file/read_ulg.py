#!/usr/bin/env -S uv run --script
# /// script
# dependencies = [
#   "pyulog",
# ]
# ///

from __future__ import annotations

import argparse

from pyulog import ULog


def main() -> int:
    parser = argparse.ArgumentParser(description="Dump key ULog content for tests")
    parser.add_argument(
        "path",
        nargs="?",
        default="out.ulg",
        help="Path to .ulg file (default: out.ulg)",
    )
    args = parser.parse_args()

    ulog = ULog(args.path)

    print(f"FILE: {args.path}")
    print("Messages")
    for entry in ulog.logged_messages:
        print(f"[{entry.timestamp}] {entry.message}")

    print("Tagged messages")
    for _, entries in ulog.logged_messages_tagged.items():
        for entry in entries:
            print(
                f"[{entry.timestamp}] tag={entry.tag} level={entry.log_level} {entry.message}"
            )

    print("Data")
    for dataset in ulog.data_list:
        print(f"name={dataset.name} multi_id={dataset.multi_id}")
        for key, values in dataset.data.items():
            print(f"  {key}: samples={values}")

    print("Initial parameters")
    for key in sorted(ulog.initial_parameters):
        print(f"{key} = {ulog.initial_parameters[key]}")

    print("Changed parameters")
    for ts, key, value in ulog.changed_parameters:
        print(f"[{ts}] {key} = {value}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
