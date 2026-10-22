# Milestone 0 — Diagnosis

Target: Logitech MX Master 3S over Bluetooth on Fedora 44 (GNOME, Wayland).

## Environment

| Item | Value | Status |
|---|---|---|
| Date | 2026-10-21 | — |
| Host | `unixflow` | CONFIRMED |
| Fedora | 44 | CONFIRMED (kernel suffix `fc44`) |
| Kernel | `7.2.11-200.fc44.x86_64` | CONFIRMED |
| BlueZ | `5.87+1.git8750129efca8-1.fc44` | CONFIRMED |
| Solaar | not installed | CONFIRMED |
| logiops | not checked | UNKNOWN |

## Device identity

| Item | Value | Status | Evidence |
|---|---|---|---|
| Transport | Bluetooth (bus `0005`), via `uhid` | CONFIRMED | sysfs path `/devices/virtual/misc/uhid/0005:046D:B034.0003` |
| Vendor ID | `0x046D` | CONFIRMED | same |
| Product ID (Bluetooth) | `0xB034` | CONFIRMED | same |
| hidraw node | `/dev/hidraw2` (number may change on reconnect) | CONFIRMED | `HID_NAME=Logitech MX Master 3S` |
| Kernel driver | `logitech-hidpp-device` | CONFIRMED | kernel log |
| HID descriptor version | `v0.06` | CONFIRMED | kernel log |
| HID++ protocol | 4.5 (HID++ 2.0 family) | CONFIRMED | kernel log: `HID++ 4.5 device connected.` |
| Bluetooth address | redacted | — | present in kernel log; do not commit |

Raw kernel log (sanitized):

```text
kernel: input: Logitech MX Master 3S as /devices/virtual/misc/uhid/0005:046D:B034.0003/input/input25
kernel: logitech-hidpp-device 0005:046D:B034.0003: input,hidraw2: BLUETOOTH HID v0.06 Mouse [Logitech MX Master 3S] on <BT-ADDR>
kernel: logitech-hidpp-device 0005:046D:B034.0003: HID++ 4.5 device connected.
```

## Findings

### F-001 — The mouse speaks HID++ over Bluetooth on this system

Status: **CONFIRMED**

The kernel driver negotiated HID++ 4.5 with the mouse over Bluetooth. The protocol channel works at the kernel level. The problem is not that the mouse or BlueZ is unable to carry HID++.

### F-002 — The user session has no access to the mouse's hidraw node

Status: **CONFIRMED**

```text
crw-------. 1 root root 241, 2 /dev/hidraw2
# file: dev/hidraw2
user::rw-
```

Only root can open the device. There is no ACL entry for the logged-in user (`user:<name>:rw-` is absent), meaning no udev rule applied `uaccess` to this node. Any non-root configuration tool (Solaar, hid-lab) will fail to open it.

### H-001 — Missing permissions is the cause of "nothing can be configured"

Status: **LIKELY** (to be confirmed by T-001 and T-002)

Consistent with F-002 and with the history "it worked partially on an earlier Fedora": the udev rules that granted access came from the Solaar package; with Solaar absent (or its rules not matching the Bluetooth `uhid` device), access is lost.

Does not explain a logiops failure, because `logid` runs as root. If logiops also failed, there is a second cause, still UNKNOWN.

## Tests

### T-001 — Apply `uaccess` rule

```bash
sudo cp 70-hid-lab-logitech.rules /etc/udev/rules.d/
sudo udevadm control --reload
sudo udevadm trigger --subsystem-match=hidraw --action=change
getfacl /dev/hidraw2 | grep user:
```

Expected: a line `user:<your-user>:rw-`.
If absent: turn the mouse off and on, then repeat `getfacl`.

Result: _pending_

### T-002 — Confirm HID++ access from user space

```bash
sudo dnf install solaar
solaar show
```

Expected: the MX Master 3S listed with its HID++ features (battery, DPI, SmartShift, etc.), run as a normal user.

Result: _pending_

Note: Solaar installs its own udev rules. Run T-001 before installing it so the result of T-001 is not masked. Uninstall Solaar after Milestone 0 to avoid conflicts with hid-lab.

## Requirements derived from this diagnosis

1. hid-lab must ship a udev rule that matches Bluetooth `uhid` devices by `KERNELS=="0005:046D:*"`, not by USB attributes.
2. The hidraw node number is not stable; the daemon must locate the device by bus/vendor/product, never by `/dev/hidrawN`.
3. On permission failure, the app must say so in plain language and offer the fix (see requirements §17).
4. Kernel driver `logitech-hidpp-device` is bound to the device; hid-lab must coexist with it (requirements §6).
