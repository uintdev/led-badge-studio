# dmgbuild settings for the macOS disk image: the app, a link to
# Applications to drag it onto, and the background with the arrow between
# them (assets/branding/dmg-background.svg, made for this layout).
#
# packaging/macos/build.sh runs dmgbuild with these, passing the paths:
#   dmgbuild -s dmg-settings.py -D app=<.app> -D background=<.tiff> <name> <.dmg>
import os.path

app = defines["app"]  # noqa: F821 (dmgbuild provides `defines`)
app_name = os.path.basename(app)

format = "UDZO"
files = [app]
symlinks = {"Applications": "/Applications"}

background = defines["background"]  # noqa: F821
# The background's size, placed a little in from the screen's corner.
window_rect = ((200, 120), (640, 400))
default_view = "icon-view"
icon_size = 128
text_size = 13
# Above the middle, so the icons and their names look centered whether or
# not Finder shows its path and status bars (which the window's height
# includes, along with the title bar).
icon_locations = {
    app_name: (160, 155),
    "Applications": (480, 155),
}
