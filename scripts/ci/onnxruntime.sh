#!/usr/bin/env bash
# ONNX Runtime from Microsoft's NuGet package, laid out as its GitHub
# release archives were: STEM/lib with the library under its versioned name
# (and libonnxruntime.so.1 on Linux, the soname), STEM/include with the
# headers. Microsoft's GitHub repository, releases and all, has answered 404
# since 17 November 2026; the NuGet package carries the same native builds
# for every desktop platform, and is pinned by digest in deps.lock like any
# other file.
#
#   scripts/ci/onnxruntime.sh RID VERSION STEM
#
# RID is NuGet's: linux-x64, linux-arm64, win-x64, win-arm64, osx-x64,
# osx-arm64. Runs on Linux, macOS and Git's bash on Windows; unpacks with
# Python, which every runner has.
set -euo pipefail

[ $# -eq 3 ] || { echo "usage: $0 RID VERSION STEM" >&2; exit 2; }
rid=$1
version=$2
stem=$3
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

"$here/fetch.sh" "onnxruntime-nuget-$version" onnxruntime.nupkg

# The first Python that runs: python3 on Linux and macOS; on Windows
# python3 can be the Store's stub, which only prints a hint.
py=
for candidate in python3 python; do
  if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c 'import zipfile' >/dev/null 2>&1; then
    py=$candidate
    break
  fi
done
[ -n "$py" ] || { echo "onnxruntime.sh: no Python to unpack with" >&2; exit 1; }

mkdir -p "$stem/lib" "$stem/include"
"$py" - onnxruntime.nupkg "$rid" "$stem" <<'EOF'
import os, shutil, sys, zipfile

package, rid, stem = sys.argv[1:4]
wanted = ((f"runtimes/{rid}/native/", "lib"), ("build/native/include/", "include"))
found = 0
with zipfile.ZipFile(package) as archive:
    for name in archive.namelist():
        for prefix, folder in wanted:
            if name.startswith(prefix) and not name.endswith("/"):
                out = os.path.join(stem, folder, name[len(prefix):])
                os.makedirs(os.path.dirname(out), exist_ok=True)
                with archive.open(name) as src, open(out, "wb") as dst:
                    shutil.copyfileobj(src, dst)
                found += folder == "lib"
if not found:
    sys.exit(f"onnxruntime.sh: the package holds nothing for {rid}")
EOF
rm -f onnxruntime.nupkg

# The names the release archives had, which the build and the bundles use.
cd "$stem/lib"
case "$rid" in
  linux-*)
    mv libonnxruntime.so "libonnxruntime.so.$version"
    ln -sf "libonnxruntime.so.$version" libonnxruntime.so.1
    ;;
  osx-*)
    mv libonnxruntime.dylib "libonnxruntime.$version.dylib"
    # The arm64 build's install name is the major version alone.
    ln -sf "libonnxruntime.$version.dylib" "libonnxruntime.${version%%.*}.dylib"
    ;;
esac
ls -l
