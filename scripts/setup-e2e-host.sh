#!/usr/bin/env bash
# Host controller for the installer's VM end-to-end cases (plan W15a).
# Boots the frozen navo-e2e base under a throwaway overlay, one cold boot per case, and collects evidence.
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: setup-e2e-host.sh <case>...      run cases, one fresh boot per case or part
       setup-e2e-host.sh --clone-base   copy the paused CI VM's disk to the frozen base
       setup-e2e-host.sh --make-base    build packs and wizards into the base (once)
Needs NAVO_WINVM, the path of the VM tool. Optional: NAVO_E2E_DIR, NAVO_E2E_SSH_PORT,
NAVO_E2E_VNC, NAVO_E2E_LOCK_WAIT (seconds, default 1800).
EOF
  exit 2
}

[[ $# -ge 1 && -n "${NAVO_WINVM:-}" ]] || usage
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
WINVM_DIR=$(dirname "$(readlink -f "$NAVO_WINVM")")
STATE=${NAVO_E2E_DIR:-$WINVM_DIR/e2e}
BASE=$STATE/navo-e2e.qcow2
VARS=$STATE/navo-e2e-vars.qcow2
RUN=$STATE/run
QMP=$RUN/qmp.sock
PIDF=$RUN/qemu.pid
PORT=${NAVO_E2E_SSH_PORT:-2223}
VNC=${NAVO_E2E_VNC:-56}
LOCK=$HOME/.cache/winvm-ci/.vm.lock
LOCK_WAIT=${NAVO_E2E_LOCK_WAIT:-1800}
CASE_TIMEOUT=1200
KEY=$HOME/.ssh/winvm_ed25519
OUT=$REPO/target/setup-e2e
SSH_OPTS=(-i "$KEY" -o BatchMode=yes -o ConnectTimeout=5 -o StrictHostKeyChecking=no
  -o "UserKnownHostsFile=$RUN/known_hosts" -o LogLevel=ERROR)

# A case that needs more than one clean start runs as parts; it passes only when every part does.
declare -A PARTS=([S9]="S9a S9b")

# Takes the CI lock on fd 9, retrying until LOCK_WAIT seconds pass; exits 75 when it stays busy.
take_lock() {
  mkdir -p "$(dirname "$LOCK")"
  exec 9>"$LOCK"
  local deadline=$((SECONDS + LOCK_WAIT))
  until flock -n 9; do
    ((SECONDS < deadline)) || { echo "setup-e2e: VM lock busy after $LOCK_WAIT s" >&2; exit 75; }
    sleep 15
  done
}

# Sends one QMP command to a socket and prints the reply; exits nonzero on a QMP error.
qmp_to() {
  python3 -I - "$1" "$2" <<'PY'
import json, socket, sys
sock = socket.socket(socket.AF_UNIX)
sock.connect(sys.argv[1])
sock.settimeout(120)
stream = sock.makefile("rw")
stream.readline()
def call(name):
    stream.write(json.dumps({"execute": name}) + "\n")
    stream.flush()
    while True:
        message = json.loads(stream.readline())
        if "return" in message or "error" in message:
            return message
call("qmp_capabilities")
reply = call(sys.argv[2])
print(json.dumps(reply))
sys.exit(1 if "error" in reply else 0)
PY
}

# Reports whether the e2e qemu of the pid file is alive; a reused pid does not count.
vm_alive() {
  [[ -f "$PIDF" ]] || return 1
  local pid
  pid=$(cat "$PIDF")
  kill -0 "$pid" 2>/dev/null && tr '\0' ' ' <"/proc/$pid/cmdline" | grep -q -- '-name winvm-e2e '
}

# Powers the e2e VM off at once, which discards a throwaway overlay; waits up to 30 s.
vm_quit() {
  vm_alive || return 0
  local pid
  pid=$(cat "$PIDF")
  qmp_to "$QMP" quit >/dev/null 2>&1 || kill "$pid" 2>/dev/null || true
  for _ in $(seq 1 60); do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 0.5
  done
  echo "setup-e2e: qemu $pid did not stop" >&2
  return 1
}

# Asks Windows to shut down and waits up to 300 s, so base writes reach the disk.
vm_shutdown() {
  local pid
  pid=$(cat "$PIDF")
  ssh_vm 'shutdown /s /t 0' || true
  for _ in $(seq 1 150); do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 2
  done
  echo "setup-e2e: guest did not shut down in 300 s" >&2
  return 1
}

# Cold-boots the base. Input: "throwaway" (writes go to a temporary qcow2 overlay) or "writable".
vm_boot() {
  local overlay=""
  [[ "$1" == throwaway ]] && overlay=",snapshot=on"
  vm_quit
  mkdir -p "$RUN" "$STATE/tmp"
  rm -f "$QMP" "$PIDF"
  # Same machine as the VM tool's, so Windows sees no new hardware. 9>&-: qemu must never keep the lock.
  TMPDIR="$STATE/tmp" nice -n 5 qemu-system-x86_64 -name winvm-e2e \
    -machine q35,accel=kvm -cpu host,hv_relaxed,hv_vapic,hv_time,hv_spinlocks=0x1fff -smp 6 -m 8G \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/x64/OVMF_CODE.4m.fd \
    -drive "if=pflash,format=qcow2,file=$VARS$overlay" \
    -device ich9-ahci,id=ahci \
    -drive "id=disk0,file=$BASE,format=qcow2,if=none,discard=unmap$overlay" \
    -device ide-hd,drive=disk0,bus=ahci.0,bootindex=1 \
    -device qemu-xhci,id=xhci -device usb-kbd,bus=xhci.0 -device usb-tablet,bus=xhci.0 \
    -netdev "user,id=n0,hostfwd=tcp:127.0.0.1:$PORT-:22" -device e1000e,netdev=n0 \
    -device VGA -rtc base=localtime -display none -vnc "127.0.0.1:$VNC" \
    -qmp "unix:$QMP,server=on,wait=off" -pidfile "$PIDF" -daemonize 9>&-
}

# Runs one command in the VM as the SSH admin account.
ssh_vm() { ssh "${SSH_OPTS[@]}" -p "$PORT" admin@127.0.0.1 "$@" 9>&-; }

# Copies files to or from the VM; the arguments are scp's.
scp_vm() { scp -q -r "${SSH_OPTS[@]}" -P "$PORT" "$@" 9>&-; }

# Waits for SSH, at most the given seconds.
wait_ssh() {
  local deadline=$((SECONDS + $1))
  until ssh_vm hostname >/dev/null 2>&1; do
    vm_alive || { echo "setup-e2e: qemu exited during boot" >&2; return 1; }
    ((SECONDS < deadline)) || { echo "setup-e2e: no SSH after $1 s" >&2; return 1; }
    sleep 3
  done
}

# Runs setup-e2e.ps1 in the VM with the given arguments, through a .ps1 file and an explicit exit.
guest_ps() {
  ssh_vm "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\e2e\\scripts\\setup-e2e.ps1 $*"
}

# Copies the current harness scripts into C:\e2e\scripts.
push_scripts() {
  ssh_vm 'cmd /c "if exist C:\e2e\scripts rmdir /s /q C:\e2e\scripts & mkdir C:\e2e\scripts"'
  scp_vm "$REPO/scripts/setup-e2e.ps1" "$REPO/setup-e2e" admin@127.0.0.1:C:/e2e/scripts/
}

# Copies the base (paused CI VM, so no guest write lands); the CI VM resumes unchanged. Holds the lock.
clone_base() {
  [[ ! -e "$BASE" ]] || { echo "setup-e2e: $BASE exists; delete it to clone again" >&2; exit 1; }
  mkdir -p "$STATE"
  take_lock
  local status before after
  status=$("$NAVO_WINVM" status 9>&-)
  if [[ "$status" == running* ]]; then
    # A global, because the EXIT trap runs after this function's locals are gone.
    ci_qmp=$WINVM_DIR/run/qmp.sock
    qmp_to "$ci_qmp" stop >/dev/null
    trap 'qmp_to "$ci_qmp" cont >/dev/null' EXIT
    # Fails closed: a copy of a disk that is still being written would be a corrupt base.
    [[ $(qmp_to "$ci_qmp" query-status) == *'"status": "paused"'* ]] ||
      { echo "setup-e2e: the CI VM did not pause" >&2; exit 1; }
  elif [[ "$status" != stopped* ]]; then
    echo "setup-e2e: unknown CI VM state: $status" >&2
    exit 1
  fi
  before=$(stat -c '%s %Y' "$WINVM_DIR/disk.qcow2")
  cp --sparse=always "$WINVM_DIR/disk.qcow2" "$BASE.part"
  cp --sparse=always "$WINVM_DIR/OVMF_VARS.qcow2" "$VARS"
  after=$(stat -c '%s %Y' "$WINVM_DIR/disk.qcow2")
  [[ "$before" == "$after" ]] || { echo "setup-e2e: the CI disk changed during the copy" >&2; exit 1; }
  sync "$BASE.part" "$VARS"
  mv "$BASE.part" "$BASE"
  sync "$STATE"
  echo "setup-e2e: base cloned to $BASE"
}

# Builds the base once: packs and wizards with writes kept, then u's first logon. No lock: own disk.
make_base() {
  [[ -f "$BASE" ]] || { echo "setup-e2e: run --clone-base first" >&2; exit 1; }
  [[ ! -e "$STATE/base.ready" ]] || { echo "setup-e2e: the base is already built" >&2; exit 1; }
  trap vm_quit EXIT
  vm_boot writable
  wait_ssh 600
  ssh_vm 'cmd /c "if not exist C:\e2e mkdir C:\e2e"'
  push_scripts
  local archive=$STATE/tmp/source.zip
  mkdir -p "$STATE/tmp"
  git -C "$REPO" archive --format=zip --output "$archive" HEAD
  scp_vm "$archive" admin@127.0.0.1:C:/e2e/source.zip
  guest_ps -MakeBase
  vm_shutdown
  vm_boot writable
  wait_ssh 600
  push_scripts
  guest_ps -WaitLogon
  vm_shutdown
  git -C "$REPO" rev-parse HEAD >"$STATE/base.ready"
  sync "$BASE" "$VARS" "$STATE/base.ready"
  echo "setup-e2e: base ready at $(cat "$STATE/base.ready")"
}

# Prints the verdict line of a finished part, or a FAIL line naming why there is none.
verdict_of() {
  local result=$OUT/$1/result.txt
  if [[ -s "$result" ]]; then head -n 1 "$result"; else echo "FAIL $1: no result in the evidence"; fi
}

# Boots a fresh overlay, runs one part in u's session, waits for done, collects the evidence.
run_part() {
  local part=$1 evidence=$OUT/$1
  rm -rf "$evidence"
  mkdir -p "$evidence"
  vm_boot throwaway
  wait_ssh 600
  push_scripts
  guest_ps -WaitLogon
  guest_ps -Launch -Case "$part"
  local deadline=$((SECONDS + CASE_TIMEOUT)) done=0
  while ((SECONDS < deadline)) && vm_alive; do
    if ssh_vm "cmd /c if exist C:\\e2e\\out\\$part\\done echo yes" 2>/dev/null | grep -q yes; then
      done=1
      break
    fi
    sleep 5
  done
  scp_vm "admin@127.0.0.1:C:/e2e/out/$part/*" "$evidence/" || true
  if ((done == 0)); then
    echo "FAIL $part: timeout after $CASE_TIMEOUT s" >"$evidence/result.txt"
  fi
  vm_quit
}

# Runs one case or all its parts and prints its PASS or FAIL line; returns 1 unless it passed.
run_case() {
  local case=$1 parts=${PARTS[$1]:-$1} part line status=0 reasons=""
  for part in $parts; do
    run_part "$part" || echo "FAIL $part: the controller stopped" >"$OUT/$part/result.txt"
    line=$(verdict_of "$part")
    [[ "$parts" == "$case" ]] || echo "  $line"
    [[ "$line" == "PASS $part"* ]] || { status=1; reasons+="$line; "; }
  done
  if ((status == 0)); then
    line="PASS $case"
  elif [[ "$parts" != "$case" ]]; then
    line="FAIL $case: ${reasons%; }"
  fi
  echo "$line" | tee -a "$OUT/summary.txt"
  return "$status"
}

# Runs a batch under the lock; the CI VM is never touched, and the overlay dies with each boot.
run_batch() {
  [[ -f "$STATE/base.ready" ]] || { echo "setup-e2e: run --make-base first" >&2; exit 1; }
  mkdir -p "$OUT"
  : >"$OUT/summary.txt"
  take_lock
  trap vm_quit EXIT
  local failed=0 case
  for case in "$@"; do
    [[ "$case" =~ ^S[0-9]+[a-z]?$ ]] || { echo "setup-e2e: bad case name $case" >&2; exit 2; }
    run_case "$case" || failed=1
  done
  echo "--- summary"
  cat "$OUT/summary.txt"
  return "$failed"
}

case "$1" in
  --clone-base) clone_base ;;
  --make-base) make_base ;;
  -*) usage ;;
  *) run_batch "$@" ;;
esac
