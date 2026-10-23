# Third-party notices

## Lucide icons

Some interface icons under `assets/icons/` are taken from [Lucide](https://github.com/lucide-icons/lucide), restyled to a 1.5 px stroke and a white root colour. Lucide is licensed under the ISC License, and the subset of its icons derived from Feather under the MIT License; the full text is in [`assets/icons/LICENSE-lucide.txt`](assets/icons/LICENSE-lucide.txt). All other icons in the set are original artwork of this project.

| File in `assets/icons/` | Lucide source icon |
|---|---|
| `shapes-annotations/emoji.svg` | `face-slightly-smiling.svg` |
| `shapes-annotations/idea.svg` | `lightbulb.svg` |
| `ui/alert.svg` | `alarm-clock.svg` |
| `ui/calendar.svg` | `calendar.svg` |
| `ui/camera.svg` | `camera.svg` |
| `ui/canvas.svg` | `palette.svg` |
| `ui/chevron-down.svg` | `chevron-down.svg` |
| `ui/close.svg` | `x.svg` |
| `ui/connected.svg` | `wifi.svg` |
| `ui/connecting.svg` | `wifi-high.svg` |
| `ui/disconnected.svg` | `wifi-off.svg` |
| `ui/drag-handle.svg` | `grip-vertical.svg` |
| `ui/error.svg` | `circle-alert.svg` |
| `ui/events.svg` | `calendar-clock.svg` |
| `ui/eye-off.svg` | `eye-off.svg` |
| `ui/eye.svg` | `eye.svg` |
| `ui/fullscreen.svg` | `maximize.svg` |
| `ui/go-to-date.svg` | `calendar-search.svg` |
| `ui/help.svg` | `circle-question-mark.svg` |
| `ui/indicators.svg` | `chart-spline.svg` |
| `ui/info.svg` | `info.svg` |
| `ui/layout-grid.svg` | `layout-grid.svg` |
| `ui/layout-setup.svg` | `layout-dashboard.svg` |
| `ui/loading.svg` | `loader-circle.svg` |
| `ui/lock.svg` | `lock.svg` |
| `ui/magnet.svg` | `magnet.svg` |
| `ui/menu.svg` | `menu.svg` |
| `ui/more.svg` | `ellipsis.svg` |
| `ui/move-pane.svg` | `arrow-up-down.svg` |
| `ui/move-vertical.svg` | `move-vertical.svg` |
| `ui/object-tree.svg` | `list-tree.svg` |
| `ui/pause.svg` | `pause.svg` |
| `ui/percent.svg` | `percent.svg` |
| `ui/play.svg` | `play.svg` |
| `ui/plus.svg` | `plus.svg` |
| `ui/redo.svg` | `redo-2.svg` |
| `ui/replay.svg` | `rewind.svg` |
| `ui/rotate-ccw.svg` | `rotate-ccw.svg` |
| `ui/scales-lines.svg` | `ruler.svg` |
| `ui/search.svg` | `search.svg` |
| `ui/settings.svg` | `settings.svg` |
| `ui/skip-back.svg` | `skip-back.svg` |
| `ui/skip-forward.svg` | `skip-forward.svg` |
| `ui/star-filled.svg` | `star.svg`, filled |
| `ui/star.svg` | `star.svg` |
| `ui/status-line.svg` | `panel-top.svg` |
| `ui/step-back.svg` | `chevron-left.svg` |
| `ui/step-forward.svg` | `chevron-right.svg` |
| `ui/stop.svg` | `square.svg` |
| `ui/success.svg` | `circle-check.svg` |
| `ui/symbol.svg` | `chart-candlestick.svg` |
| `ui/trash.svg` | `trash.svg` |
| `ui/undo.svg` | `undo-2.svg` |
| `ui/unlock.svg` | `lock-open.svg` |
| `ui/warning.svg` | `triangle-alert.svg` |
| `ui/watchlist.svg` | `list-plus.svg` |
| `ui/zoom-in.svg` | `zoom-in.svg` |
| `ui/zoom-out.svg` | `zoom-out.svg` |

## Roboto

`assets/fonts/Roboto-Latin.ttf` is Roboto 3.015 from the [upstream release](https://github.com/googlefonts/roboto-3-classic/releases/tag/v3.015), embedded by the chart widget. It is limited to weights 400 to 600 at normal width and subset to Latin, with punctuation, currency, arrows, math and interface markers. Roboto is licensed under the SIL Open Font License 1.1; the full text is in [`assets/fonts/LICENSE-roboto.txt`](assets/fonts/LICENSE-roboto.txt).

The source is `web/split/Roboto[wdth,wght].ttf` in `Roboto_v3.015.zip` (SHA-256 `d7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134` for the TTF). The subset retains hinting and the source timestamp.

## Fonts in the web demo

The library embeds Roboto. The web demo under `examples/demo` also enables egui's `default_fonts` feature through `eframe`, so its WebAssembly module contains egui's default fonts: Hack Regular (MIT License and Bitstream Vera License), Ubuntu Light (Ubuntu Font Licence 1.0) and egui-icons, a five-glyph subset of emoji-icon-font (MIT License). The demo also enables egui's `monochrome_emoji_fonts` feature for the drawing bar's icon and emoji menu, which adds Noto Emoji Regular (SIL Open Font License 1.1) and the full emoji-icon-font (MIT License). Their notices and licences are in [`examples/demo/LICENSE-fonts.txt`](examples/demo/LICENSE-fonts.txt). The demo's build publishes that file and the Lucide licence next to the demo page.
