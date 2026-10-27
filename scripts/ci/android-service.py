#!/usr/bin/env python3
"""Declares the run service in the APK cargo-apk built, and adds its code.

    android-service.py --apk LongCut.apk --manifest AndroidManifest.xml
                       --res RES --dex classes.dex --aapt AAPT
                       --android-jar ANDROID_JAR --out UNALIGNED.apk

A Long video run goes on with the app in the background through a
foreground service, java/RunService.java in concat-android. cargo-apk
writes no <service> into the manifest and no classes.dex into the APK -
its activity is the framework's NativeActivity, with no code of its own -
so this does both, after `cargo apk build`:

  - the manifest cargo-apk wrote (the text one, beside the APK) gets the
    service and `android:hasCode="true"`, and is packaged again with its
    resources by aapt, the way cargo-apk packaged it;
  - everything else the built APK carries - the native libraries - is
    copied over as it was, and the dex build.rs made goes in as
    classes.dex, where the system loads the service from.

What comes out is unaligned and unsigned: zipalign and apksigner follow
(mobile.yml). Fails, rather than writing a second service, on a manifest
that already declares one.
"""
import argparse
import pathlib
import re
import subprocess
import sys
import tempfile
import zipfile

SERVICE = (
    '<service android:name="app.concat.editor.RunService"'
    ' android:foregroundServiceType="dataSync"'
    ' android:exported="false"/>'
)


def patch(manifest: str) -> str:
    """The manifest with the service declared and the APK's code on."""
    if "RunService" in manifest:
        sys.exit("android-service: the manifest already declares the run service")
    if "</application>" not in manifest:
        sys.exit("android-service: the manifest has no </application> to declare it in")
    if re.search(r'android:hasCode="[^"]*"', manifest):
        manifest = re.sub(r'android:hasCode="[^"]*"', 'android:hasCode="true"', manifest, count=1)
    else:
        manifest = manifest.replace("<application", '<application android:hasCode="true"', 1)
    return manifest.replace("</application>", SERVICE + "</application>", 1)


def merge(packaged: pathlib.Path, built: pathlib.Path, dex: pathlib.Path, out: pathlib.Path):
    """The newly packaged manifest and resources, everything else the built
    APK held but its signature, and the dex."""
    with zipfile.ZipFile(packaged) as fresh, zipfile.ZipFile(built) as old, zipfile.ZipFile(
        out, "w"
    ) as apk:
        names = set()
        for info in fresh.infolist():
            apk.writestr(info, fresh.read(info))
            names.add(info.filename)
        for info in old.infolist():
            name = info.filename
            if name in names or name.startswith("META-INF/") or name == "classes.dex":
                continue
            apk.writestr(info, old.read(info))
            names.add(name)
        apk.write(dex, "classes.dex", compress_type=zipfile.ZIP_DEFLATED)
        return sorted(names | {"classes.dex"})


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    for flag in ("apk", "manifest", "res", "dex", "aapt", "android-jar", "out"):
        parser.add_argument(f"--{flag}", required=True, type=pathlib.Path)
    args = parser.parse_args()

    with tempfile.TemporaryDirectory() as scratch:
        scratch = pathlib.Path(scratch)
        manifest = scratch / "AndroidManifest.xml"
        manifest.write_text(patch(args.manifest.read_text(encoding="utf-8")), encoding="utf-8")
        packaged = scratch / "packaged.apk"
        subprocess.run(
            [
                str(args.aapt), "package", "-f",
                "-F", str(packaged),
                "-M", str(manifest),
                "-I", str(args.android_jar),
                "-S", str(args.res),
            ],
            check=True,
        )
        names = merge(packaged, args.apk, args.dex, args.out)

    libs = [name for name in names if name.startswith("lib/")]
    if not libs:
        sys.exit("android-service: the built APK carried no native library")
    print(f"android-service: the run service declared; {len(names)} entries, {len(libs)} libraries")


if __name__ == "__main__":
    main()
