#!/bin/bash
# Repackages the stock-layout deb from build.py as `opendesk`, which installs beside
# official RustDesk: every path, the command, the systemd unit, desktop entries, icons and the
# URI scheme become `opendesk`, matching the app name the binary sets at startup
# (src/common.rs FORK_APP_NAME, lowercased).
#
# usage: rebrand-deb.sh <stock.deb> <out.deb>
set -euo pipefail

in=$(realpath "$1")
out=$(realpath -m "$2")
old=rustdesk
new=opendesk

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
dpkg-deb -R "$in" "$work/pkg"
cd "$work/pkg"

mv "usr/share/$old" "usr/share/$new"
mv "usr/share/$new/$old" "usr/share/$new/$new"
mv "usr/share/$new/files/systemd/$old.service" "usr/share/$new/files/systemd/$new.service"
mv "usr/share/applications/$old.desktop" "usr/share/applications/$new.desktop"
mv "usr/share/applications/$old-link.desktop" "usr/share/applications/$new-link.desktop"
mv "usr/share/icons/hicolor/256x256/apps/$old.png" "usr/share/icons/hicolor/256x256/apps/$new.png"
mv "usr/share/icons/hicolor/scalable/apps/$old.svg" "usr/share/icons/hicolor/scalable/apps/$new.svg"

# Every lowercase `rustdesk` in these files names a path, the command, the unit, the icon, the
# WM class or the URI scheme; none of them contains `opendesk` yet.
text_files=(
  "usr/share/$new/files/systemd/$new.service"
  "usr/share/applications/$new.desktop"
  "usr/share/applications/$new-link.desktop"
)
for f in DEBIAN/preinst DEBIAN/postinst DEBIAN/prerm DEBIAN/postrm; do
  [ -f "$f" ] && text_files+=("$f")
done
sed -i "s/$old/$new/g" "${text_files[@]}"

sed -i 's/^Name=RustDesk$/Name=OpenDesk/' \
  "usr/share/applications/$new.desktop" "usr/share/applications/$new-link.desktop"
sed -i 's/^Description=RustDesk$/Description=OpenDesk (unofficial RustDesk fork)/' \
  "usr/share/$new/files/systemd/$new.service"

sed -i \
  -e 's/^Package: rustdesk$/Package: opendesk/' \
  -e 's|^Maintainer: .*|Maintainer: Ronil999 <Ronil999@users.noreply.github.com>|' \
  -e 's|^Homepage: .*|Homepage: https://github.com/Ronil999/rustdesk|' \
  -e 's|^Description: .*|Description: OpenDesk, an unofficial RustDesk fork with Blackout. Installs beside official RustDesk.|' \
  DEBIAN/control

chmod -R u+rwX,go+rX,go-w usr
find usr -type f -print0 | sort -z | xargs -0 md5sum > DEBIAN/md5sums

cd "$work"
dpkg-deb --root-owner-group -b pkg "$out" >/dev/null

# Nothing may overlap with the official package.
if dpkg-deb -c "$out" | awk '{print $NF}' |
  grep -E "/$old(/|$)|/$old(-link)?\.(desktop|service|png|svg)$|^\./usr/bin/$old$"; then
  echo "rebrand-deb: paths above still collide with official RustDesk" >&2
  exit 1
fi
if grep -Eq "(^|[^a-z])$old([^a-z]|$)" "$work/pkg/DEBIAN/postinst" "$work/pkg/DEBIAN/prerm"; then
  echo "rebrand-deb: maintainer scripts still name $old" >&2
  exit 1
fi
dpkg-deb --field "$out" Package Version
