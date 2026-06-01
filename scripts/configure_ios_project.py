#!/usr/bin/env python3
"""Patch the generated iOS Xcode project (``src-tauri/gen/apple``).

``cargo tauri ios init`` regenerates ``gen/apple`` from a template, so the
App-Store-only bits Tauri does not emit must be re-applied after each init.
``just ios-init`` runs this (idempotent). It performs four edits:

1. ``NSFaceIDUsageDescription`` in the app Info.plist. Without it, iOS reports
   Face ID unavailable to ``LAContext.canEvaluatePolicy``, so the in-app lock's
   availability check fails and the lock silently disengages (Android needs no
   such string, which is why it works there).
2. The App Group ``group.com.luminaapps.talea`` on the app entitlements, so
   the app can write the shared container the widget reads.
3. A ``TaleaWidget`` app-extension target (from ``ios-widget/``) embedded
   into the app, so the home-screen widget ships.
4. ``buildPhase: none`` on the app's ``Externals`` source, so the Rust
   ``libapp.a`` links but isn't copied into the bundle (App Store rejects a
   standalone library inside the .app).

It then regenerates the ``.xcodeproj`` with ``xcodegen`` (an iOS prerequisite).

Requires PyYAML (``python3 -m pip install pyyaml``); ``plistlib`` is stdlib.
"""

from __future__ import annotations

import os
import plistlib
import subprocess
import sys

import yaml

APP_TARGET = "talea_iOS"
WIDGET_TARGET = "TaleaWidget"
WIDGET_BUNDLE_ID = "com.luminaapps.talea.TaleaWidget"
APP_GROUP = "group.com.luminaapps.talea"
FACE_ID_REASON = "Unlock Talea with Face ID."
# ios-widget/ sits at the repo root; gen/apple is three levels down.
WIDGET_SRC = "../../../ios-widget/TaleaWidget"


def resolve_team(app_target: dict) -> str | None:
    """The Apple development team to sign the widget with.

    Prefer the env var the recipes export; otherwise reuse whatever team Tauri
    injected into the app target.
    """
    team = os.environ.get("APPLE_DEVELOPMENT_TEAM")
    if team:
        return team
    base = app_target.get("settings", {}).get("base", {})
    return base.get("DEVELOPMENT_TEAM")


def widget_target(team: str | None, short: str, build: str) -> dict:
    """The XcodeGen spec for the WidgetKit app-extension target.

    ``short``/``build`` are the app's versions; the extension must match them
    (App Store validation), and its Info.plist references them via
    ``$(MARKETING_VERSION)`` / ``$(CURRENT_PROJECT_VERSION)``.
    """
    settings = {
        "PRODUCT_NAME": WIDGET_TARGET,
        "PRODUCT_BUNDLE_IDENTIFIER": WIDGET_BUNDLE_ID,
        "INFOPLIST_FILE": f"{WIDGET_SRC}/Info.plist",
        "CODE_SIGN_ENTITLEMENTS": f"{WIDGET_SRC}/TaleaWidget.entitlements",
        "CODE_SIGN_STYLE": "Automatic",
        "GENERATE_INFOPLIST_FILE": "NO",
        "IPHONEOS_DEPLOYMENT_TARGET": "17.0",
        "SWIFT_VERSION": "5.0",
        "TARGETED_DEVICE_FAMILY": "1,2",
        "SKIP_INSTALL": "YES",
        "MARKETING_VERSION": short,
        "CURRENT_PROJECT_VERSION": build,
    }
    if team:
        settings["DEVELOPMENT_TEAM"] = team
    return {
        "type": "app-extension",
        "platform": "iOS",
        "sources": [
            {
                "path": WIDGET_SRC,
                "excludes": ["Info.plist", "*.entitlements"],
            }
        ],
        "settings": {"base": settings},
    }


def unbundle_externals(app_target: dict) -> None:
    """Mark the app's ``Externals`` source as file-reference-only.

    XcodeGen otherwise copies the Rust ``libapp.a`` in it into the app bundle,
    which App Store rejects; ``buildPhase: none`` keeps it out (it still links
    via the ``framework: libapp.a`` dependency + ``LIBRARY_SEARCH_PATHS``).
    """
    sources = app_target.get("sources") or []
    for index, src in enumerate(sources):
        path_val = src.get("path") if isinstance(src, dict) else src
        if path_val != "Externals":
            continue
        if isinstance(src, dict):
            src["buildPhase"] = "none"
        else:
            sources[index] = {"path": "Externals", "buildPhase": "none"}


def patch_project_yaml(path: str) -> None:
    """Add the Face ID string and the embedded widget target to project.yml."""
    with open(path, encoding="utf-8") as handle:
        data = yaml.safe_load(handle)

    targets = data.setdefault("targets", {})
    app = targets.get(APP_TARGET)
    if app is None:
        raise SystemExit(f"error: target {APP_TARGET!r} not found in {path}")

    # 1. Face ID consent string on the app Info.plist.
    info = app.setdefault("info", {})
    props = info.setdefault("properties", {})
    props["NSFaceIDUsageDescription"] = FACE_ID_REASON

    # 3. The widget target (versions matched to the app) + embed it.
    short = str(props.get("CFBundleShortVersionString", "1.0.0"))
    build = str(props.get("CFBundleVersion", "1.0.0"))
    targets[WIDGET_TARGET] = widget_target(resolve_team(app), short, build)
    deps = app.setdefault("dependencies", [])
    embedded = any(
        isinstance(d, dict) and d.get("target") == WIDGET_TARGET for d in deps
    )
    if not embedded:
        deps.append({"target": WIDGET_TARGET, "embed": True})

    # 4. Keep libapp.a out of the app bundle. The Externals source holds the
    # Rust staticlib; XcodeGen would copy it into the .app, which App Store
    # rejects ("standalone … library not permitted"). Mark it file-reference
    # only — it still links via the `framework: libapp.a` dependency above plus
    # the per-arch LIBRARY_SEARCH_PATHS.
    unbundle_externals(app)

    # safe_dump normalizes YAML 1.1 booleans; XcodeGen re-normalizes them, and
    # the settings we add are quoted "NO"/"YES" strings, so this is safe.
    with open(path, "w", encoding="utf-8") as handle:
        yaml.safe_dump(data, handle, sort_keys=False, default_flow_style=False)


def patch_app_entitlements(path: str) -> bool:
    """Add the App Group to the app entitlements plist. Returns False if the
    file is absent (Tauri may name it differently)."""
    if not os.path.exists(path):
        return False
    with open(path, "rb") as handle:
        data = plistlib.load(handle)
    groups = data.setdefault("com.apple.security.application-groups", [])
    if APP_GROUP not in groups:
        groups.append(APP_GROUP)
    with open(path, "wb") as handle:
        plistlib.dump(data, handle)
    return True


def main() -> int:
    """Patch gen/apple and regenerate the Xcode project."""
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    gen = os.path.join(repo_root, "src-tauri", "gen", "apple")
    project_yml = os.path.join(gen, "project.yml")
    if not os.path.exists(project_yml):
        sys.stderr.write(
            "error: src-tauri/gen/apple/project.yml not found "
            "(run `cargo tauri ios init` first)\n"
        )
        return 1

    patch_project_yaml(project_yml)
    print(
        "patched project.yml: NSFaceIDUsageDescription + TaleaWidget target "
        "+ Externals unbundled"
    )

    entitlements = os.path.join(gen, APP_TARGET, f"{APP_TARGET}.entitlements")
    if not patch_app_entitlements(entitlements):
        sys.stderr.write(
            f"error: {entitlements} not found "
            "(run `cargo tauri ios init` first); the widget can't sign "
            f"without the App Group {APP_GROUP}\n"
        )
        return 1
    print(f"patched {APP_TARGET}.entitlements: App Group")

    try:
        subprocess.run(
            ["xcodegen", "generate", "--spec", "project.yml"],
            cwd=gen,
            check=True,
            timeout=120,
        )
    except FileNotFoundError:
        sys.stderr.write(
            "warning: xcodegen not found; run it in src-tauri/gen/apple "
            "(or a cli build) to apply the project.yml changes\n"
        )
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as err:
        sys.stderr.write(f"error: xcodegen failed: {err}\n")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
