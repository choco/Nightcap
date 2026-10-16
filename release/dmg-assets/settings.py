# dmgbuild's settings for Nightcap's DMG: the app beside a link to /Applications, on the
# brand's background. release/dmg passes the app and the background (a 1x and 2x TIFF).
import os.path

app = defines["app"]

format = "ULFO"
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = os.path.join(app, "Contents/Resources/AppIcon.icns")

background = defines["background"]
window_rect = ((200, 140), (640, 400))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
show_icon_preview = False

# The arrow in background.html runs between these two.
icon_size = 112
text_size = 13
icon_locations = {os.path.basename(app): (160, 205), "Applications": (480, 205)}
