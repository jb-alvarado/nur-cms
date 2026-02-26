#!/usr/bin/env python3

import os
import json
import sys
import uuid
from pathlib import Path
from dotenv import load_dotenv

import requests

load_dotenv()

API_URL = os.getenv("PYTHON_API_URL")
LOGIN = json.loads(os.getenv("PYTHON_LOGIN"))
IMAGE_PATH = os.getenv("IMAGE_PATH")
CHUNK_SIZE = 5 * 1024 * 1024


def auth():
    req = requests.post(f"{API_URL}/auth/login", json=LOGIN)
    req.raise_for_status()
    token = req.json()
    access = token.get("access")
    if not access:
        raise RuntimeError("Missing access token from auth response")
    return access


def auth_headers(access_token: str) -> dict:
    return {
        "Authorization": f"Bearer {access_token}",
    }


def fetch_authors(access_token: str):
    url = f"{API_URL}/api/content/authors"
    response = requests.get(
        url,
        headers=auth_headers(access_token),
        timeout=10,
    )
    response.raise_for_status()
    payload = response.json()

    if isinstance(payload, list):
        return payload

    results = payload.get("results")

    return results


def resolve_image_path(image_path: str) -> Path | None:
    if not image_path:
        return None

    relative = image_path.lstrip("/")
    absolut_path = Path(IMAGE_PATH).joinpath(relative)

    if absolut_path.is_file():
        return absolut_path

    return None


def select_media_by_filename(filename: str, access_token: str) -> dict | None:
    response = requests.get(
        f"{API_URL}/api/media",
        headers=auth_headers(access_token),
        params={
            "search": filename,
            "ordering": "id",
            "limit": 10,
            "fields": "id,filename,path",
        },
        timeout=20,
    )
    response.raise_for_status()
    payload = response.json()
    results = payload.get("results") or []

    for media in results:
        if media.get("filename") == filename:
            return media

    return results[0] if results else None


def upload_file_in_chunks(file_path: str, access_token: str):
    file_name = os.path.basename(file_path)
    size = os.path.getsize(file_path)

    batch_id = str(uuid.uuid4())
    batch_count = 1

    with open(file_path, "rb") as f:
        start = 0
        while start < size:
            chunk = f.read(CHUNK_SIZE)
            end = start + len(chunk)

            data = {
                "fileName": file_name,
                "start": str(start),
                "end": str(end),
                "size": str(size),
                "batch_id": batch_id,
                "batch_count": str(batch_count),
            }

            files = {
                "chunk": (file_name, chunk, "application/octet-stream"),
            }

            r = requests.post(
                f"{API_URL}/api/upload",
                data=data,
                files=files,
                headers=auth_headers(access_token),
                timeout=120,
            )
            if r.status_code != 200:
                raise RuntimeError(
                    f"Upload failed at {start}-{end}: {r.status_code} {r.text}"
                )

            start = end

    print("OK: upload complete")


def upload_media_and_update(media_path: Path, access_token: str) -> int:
    filename = media_path.name
    media = select_media_by_filename(filename, access_token)

    if media is None:
        upload_file_in_chunks(media_path, access_token)

    media = select_media_by_filename(filename, access_token)
    if not media or not media.get("id"):
        raise RuntimeError(f"Uploaded media not found for filename: {filename}")

    media_id = int(media["id"])

    return media_id


def update_author(payload: dict, author_id: int, access_token: str):
    requests.put(
        f"{API_URL}/api/content/authors/{author_id}",
        headers={**auth_headers(access_token), "Content-Type": "application/json"},
        json=payload,
        timeout=20,
    ).raise_for_status()


def process_authors(authors: list, access_token: str):
    db_authors = fetch_authors(access_token) or []

    for db_author in db_authors:
        name = f"{db_author['first_name']} {db_author['last_name']}"

        for author in authors:
            if author["name"] == name:
                if (
                    db_author.get("media_id") is None
                    and author.get("image") is not None
                ):
                    image_path = resolve_image_path(author.get("image"))
                    if image_path is None:
                        print(f"Image not found for {name}: {author.get('image')}")
                        continue

                    media_id = upload_media_and_update(
                        image_path,
                        access_token,
                    )

                    payload = {"media_id": media_id}

                    if db_author.get("bio") is None:
                        payload["bio"] = author.get("description")

                    update_author(payload, int(db_author["id"]), access_token)
                    print(f"Updated author {name} with media_id={media_id}")

                elif (
                    db_author.get("bio") is None
                    and author.get("description") is not None
                ):
                    payload = {"bio": author.get("description")}

                    update_author(payload, int(db_author["id"]), access_token)
                    print(f"Updated author {name} bio")


def main() -> int:
    access_token = auth()

    if len(sys.argv) < 2:
        print("Usage: sync_authors.py /path/to/authors.json", file=sys.stderr)
        return 1

    author_path = sys.argv[1]

    if not Path(author_path).exists():
        print(f"authors json not found: {author_path}", file=sys.stderr)
        return 1

    with open(author_path, "r") as file:
        data = json.load(file)

    process_authors(data, access_token)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
