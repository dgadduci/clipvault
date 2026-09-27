#!/usr/bin/env python3
"""Send an "LLM completed" notification to ntfy.sh channel `diego_adduci`.

Usage:
    python3 scripts/notify_llm_completed.py
    python3 scripts/notify_llm_completed.py "Custom message body"
    python3 scripts/notify_llm_completed.py "Message" --priority high
    NTFY_TOPIC=other_channel python3 scripts/notify_llm_completed.py "Msg"

Environment variables (optional):
    NTFY_SERVER   Base URL of the ntfy server (default: https://ntfy.sh)
    NTFY_TOPIC    Topic to publish to (default: diego_adduci)
    NTFY_TITLE    Notification title (default: "LLM completed")
    NTFY_TOKEN    Access token if the topic requires auth
"""

from __future__ import annotations

import argparse
import os
import sys
import urllib.error
import urllib.request


def send_notification(
    message: str,
    *,
    server: str,
    topic: str,
    title: str,
    priority: str | None,
    token: str | None,
) -> None:
    url = f"{server.rstrip('/')}/{topic}"
    headers = {
        "Title": title,
        "Content-Type": "text/plain; charset=utf-8",
    }
    if priority:
        headers["Priority"] = priority
    if token:
        headers["Authorization"] = f"Bearer {token}"

    data = message.encode("utf-8")
    request = urllib.request.Request(
        url,
        data=data,
        headers=headers,
        method="POST",
    )

    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            body = response.read().decode("utf-8", errors="replace")
            print(f"Sent to {url} (HTTP {response.status}).", file=sys.stderr)
            if body:
                print(body, file=sys.stderr)
    except urllib.error.HTTPError as exc:
        error_body = exc.read().decode("utf-8", errors="replace")
        print(
            f"ntfy returned HTTP {exc.code} {exc.reason}: {error_body}",
            file=sys.stderr,
        )
        raise SystemExit(1)
    except urllib.error.URLError as exc:
        print(f"Network error contacting {url}: {exc.reason}", file=sys.stderr)
        raise SystemExit(1)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Publish a notification to ntfy.sh.",
    )
    parser.add_argument(
        "message",
        nargs="?",
        default="The LLM task finished. Ready for review.",
        help="Message body (default: %(default)s)",
    )
    parser.add_argument(
        "--priority",
        choices=["max", "high", "default", "low", "min"],
        help="ntfy priority header",
    )
    parser.add_argument(
        "--server",
        default=os.environ.get("NTFY_SERVER", "https://ntfy.sh"),
        help="ntfy server base URL (default: %(default)s)",
    )
    parser.add_argument(
        "--topic",
        default=os.environ.get("NTFY_TOPIC", "diego_adduci"),
        help="Topic/channel name (default: %(default)s)",
    )
    parser.add_argument(
        "--title",
        default=os.environ.get("NTFY_TITLE", "LLM completed"),
        help='Notification title (default: "%(default)s")',
    )
    parser.add_argument(
        "--token",
        default=os.environ.get("NTFY_TOKEN"),
        help="Optional access token (or set NTFY_TOKEN)",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    send_notification(
        message=args.message,
        server=args.server,
        topic=args.topic,
        title=args.title,
        priority=args.priority,
        token=args.token,
    )


if __name__ == "__main__":
    main()
