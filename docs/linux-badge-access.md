# Badge access on Linux

Linux doesn't let normal users write to USB devices unless a udev rule allows
it. When a badge is plugged in but not yet writable, the app shows a bar with
an **Enable Badge Access** button. It asks for your password (via polkit),
installs the rule and reloads udev. This is needed once per computer.

The light at the left of the title bar shows the selected badge's state at
a glance (hover over it for details):

| Light | Meaning |
|---|---|
| Green | A badge is connected and ready to send to |
| Amber | A badge is connected, but your account can't write to it yet: set up badge access |
| Red | No badge is connected |

It updates when a badge is plugged in or removed, when you pick another
badge, after setting up access, and when you switch back to the window (for
example after installing the rule in a terminal). The app draws its own title
bar only on Linux; on Windows and macOS, where no setup is needed, the
system's title bar is used.

**File → Badge Access Help…** shows whether the rule is installed and has
buttons to **Install / Reinstall** or **Remove** it. The command line can do
the same:

```bash
badgectl udev status      # is the rule installed, and is the badge writable?
badgectl udev install     # install or reinstall (asks for your password)
badgectl udev remove      # remove it again
```

To do it by hand instead:

```bash
badgectl udev | sudo tee /etc/udev/rules.d/70-led-badge.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Removing the rule stops new access. A badge that's already plugged in may stay
writable until it's unplugged. `./packaging/linux/install.sh --remove-udev`
also removes it.

The file name must sort before systemd's `73-seat-late.rules` (hence `70-`),
or the `uaccess` tag that grants access to the logged-in user is never applied.

**In the Flatpak**, the button and `badgectl udev` work the same way: the
Flatpak asks your system to run the install command, which shows the usual
password prompt. A Flatpak built without the host-command permission can't
do that; the dialog then says the rule can't be checked. It shows the
commands to install and to remove the rule by hand, each with a **Copy**
button, to paste into a terminal.

**Immutable distributions** (SteamOS, Bazzite, Fedora Silverblue/Kinoite and
similar): `/etc` is writable and survives updates, so the same rule works. On
SteamOS, set a password for the `deck` user first (`passwd` in Konsole, Desktop
Mode), since the password prompt needs one. On NixOS, add the rule to
`services.udev.extraRules`.

The desktop USB portal can't replace this step: it only hands out the access
your user already has.
