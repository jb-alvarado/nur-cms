#!/usr/bin/env python3

import os
import json
import sqlite3
import sys
from pathlib import Path
from dotenv import load_dotenv

import requests

load_dotenv()

API_URL = os.getenv("PYTHON_API_URL")
LOGIN = json.loads(os.getenv("PYTHON_LOGIN"))


def auth():
    req = requests.post(f"{API_URL}/auth/login", json=LOGIN)
    req.raise_for_status()
    token = req.json()
    access = token.get("access")
    if not access:
        raise RuntimeError("Missing access token from auth response")
    return access


def fetch_entry_id_by_slug(slug: str):
    url = f"{API_URL}/api/content/entries"
    response = requests.get(url, params={"slug": slug, "fields": "id"}, timeout=5)
    response.raise_for_status()
    payload = response.json()

    if isinstance(payload, list):
        return payload[0] if payload else None

    results = payload.get("results") or []
    if not results:
        return None
    return results[0].get("id")


def get_existing_comments(slug: str, access_token: dict):
    response = requests.get(
        f"{API_URL}/api/comments?slug={slug}",
        headers={"Authorization": f"Bearer {access_token}"},
        timeout=10,
    )
    response.raise_for_status()
    payload = response.json()

    results = payload.get("results") or []
    if not results:
        return None
    return results


def comment_key(comment: dict) -> tuple:
    return (
        comment.get("author_name"),
        comment.get("author_email"),
        comment.get("created_at"),
    )


def process_entries(data: list, access_token: dict):
    id_map: dict[int, int] = {}

    for d in data:
        entry_id = fetch_entry_id_by_slug(d["slug"])
        comments = get_existing_comments(d["slug"], access_token) or []
        exists = False

        for comment in comments:
            if (
                d.get("author_name") == comment.get("author_name")
                and d.get("author_email") == comment.get("author_email")
                and d.get("text") == comment.get("text")
            ):
                print(f"Comment already exists for slug={d['slug']}, skipping")
                exists = True
                break

        if exists:
            continue

        if not entry_id:
            print(f"No entry found for slug: {d['slug']}", file=sys.stderr)
            continue

        d["entry_id"] = entry_id

        old_id = d["id"]
        parent_id = d.get("parent_id")
        if parent_id is not None:
            new_parent_id = id_map.get(parent_id)
            if new_parent_id is None:
                print(
                    f"Missing parent mapping for id={old_id}, parent_id={parent_id}",
                    file=sys.stderr,
                )
                continue
            d["parent_id"] = new_parent_id

        del d["id"]
        del d["slug"]

        try:
            req = requests.post(
                f"{API_URL}/api/comments",
                json=d,
                headers={"Authorization": f"Bearer {access_token}"},
                timeout=10,
            )
            req.raise_for_status()
        except requests.exceptions.HTTPError:
            print(f"Comment insert failed: {d['slug']} | {d['author_name']}", file=sys.stderr)
            print(req.status_code, file=sys.stderr)
            print(req.text, file=sys.stderr)
            exit(1)

        if not req.content:
            print("Empty response body for comment insert", file=sys.stderr)
            print(req.status_code, file=sys.stderr)
            print(req.text, file=sys.stderr)
            exit(1)

        try:
            new_id = req.json()
        except requests.exceptions.JSONDecodeError:
            print("Non-JSON response body for comment insert", file=sys.stderr)
            print(req.status_code, file=sys.stderr)
            print(req.text, file=sys.stderr)
            exit(1)

        id_map[old_id] = new_id


def main() -> int:
    access_token = auth()

    if len(sys.argv) < 2:
        print("Usage: comments_from_isso.py /path/to/isso.db", file=sys.stderr)
        return 1

    dbpath = sys.argv[1]

    if not Path(dbpath).exists():
        print(f"DB not found: {dbpath}", file=sys.stderr)
        return 1

    query = """
    SELECT
      trim(replace(threads.uri, '/news/', ''), '/') AS slug,
      comments.id            AS id,
      comments.parent        AS parent_id,
      CASE WHEN comments.mode = 1 THEN 'approved' ELSE 'pending' END AS status,
            strftime('%Y-%m-%dT%H:%M:%S.000Z', comments.created, 'unixepoch') AS created_at,
            ifnull(
                strftime('%Y-%m-%dT%H:%M:%S.000Z', comments.modified, 'unixepoch'),
                strftime('%Y-%m-%dT%H:%M:%S.000Z', comments.created, 'unixepoch')
            ) AS updated_at,
      trim(comments.author)  AS author_name,
      trim(comments.email)   AS author_email,
      trim(comments.text)    AS text
    FROM comments
    INNER JOIN threads ON comments.tid = threads.id
    ORDER BY comments.tid, comments.id
    """

    con = sqlite3.connect(dbpath)
    con.row_factory = sqlite3.Row
    try:
        rows = con.execute(query).fetchall()
        data = [dict(row) for row in rows]
    finally:
        con.close()

    process_entries(data, access_token)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
