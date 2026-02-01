#!/usr/bin/env bash
# shellcheck disable=SC2034
# Octo Archiso Profile Definition
# Build with: sudo mkarchiso -v -w /tmp/archiso-tmp -o ~/octo-iso .

iso_name="octo-arch"
iso_label="OCTO_$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y%m)"
iso_publisher="Octo <https://github.com/byteowlz/octo>"
iso_application="Octo AI Agent Platform"
iso_version="$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y.%m.%d)"
install_dir="arch"
buildmodes=('iso')
bootmodes=('bios.syslinux'
           'uefi.systemd-boot')
arch="x86_64"
pacman_conf="pacman.conf"
airootfs_image_type="squashfs"
airootfs_image_tool_options=('-comp' 'xz' '-Xbcj' 'x86' '-b' '1M' '-Xdict-size' '1M')
file_permissions=(
  ["/etc/shadow"]="0:0:400"
  ["/root"]="0:0:750"
  ["/root/.automated_script.sh"]="0:0:755"
  # Scripts
  ["/usr/local/bin/octo-install"]="0:0:755"
  ["/usr/local/bin/octo-setup"]="0:0:755"
  ["/usr/local/bin/octo-first-boot"]="0:0:755"
  # Core Octo binaries
  ["/usr/local/bin/octo"]="0:0:755"
  ["/usr/local/bin/octoctl"]="0:0:755"
  ["/usr/local/bin/octo-runner"]="0:0:755"
  ["/usr/local/bin/octo-files"]="0:0:755"
  ["/usr/local/bin/octo-sandbox"]="0:0:755"
  ["/usr/local/bin/pi-bridge"]="0:0:755"
  # Agent tools
  ["/usr/local/bin/mmry"]="0:0:755"
  ["/usr/local/bin/trx"]="0:0:755"
  ["/usr/local/bin/agntz"]="0:0:755"
  ["/usr/local/bin/hstry"]="0:0:755"
  ["/usr/local/bin/byt"]="0:0:755"
  ["/usr/local/bin/mailz"]="0:0:755"
  # Search tools
  ["/usr/local/bin/sx"]="0:0:755"
  ["/usr/local/bin/scrpr"]="0:0:755"
  # LLM tools
  ["/usr/local/bin/eavs"]="0:0:755"
  ["/usr/local/bin/skdlr"]="0:0:755"
  ["/usr/local/bin/tmpltr"]="0:0:755"
  # Communication
  ["/usr/local/bin/h8"]="0:0:755"
  # Media tools
  ["/usr/local/bin/sldr"]="0:0:755"
  ["/usr/local/bin/kokorox"]="0:0:755"
  ["/usr/local/bin/ears"]="0:0:755"
  # Other tools
  ["/usr/local/bin/dgrmr"]="0:0:755"
  ["/usr/local/bin/cmfy"]="0:0:755"
  ["/usr/local/bin/hmr"]="0:0:755"
  ["/usr/local/bin/ignr"]="0:0:755"
  ["/usr/local/bin/ingestr"]="0:0:755"
)
