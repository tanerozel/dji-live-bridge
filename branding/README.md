# StreamMyDrone brand assets

The primary mark combines three product cues: a drone, a suspended camera with a play symbol, and a wireless broadcast signal.

## Files

- `streammydrone-app-icon.svg`: master full-color app icon.
- `streammydrone-play-store.svg`: full-square Google Play source; Google applies the mask.
- `streammydrone-play-store-512.png`: Google Play store icon (512 x 512, 32-bit PNG).
- `streammydrone-mark-light.svg`: transparent mark for dark backgrounds.
- `streammydrone-mark-dark.svg`: transparent mark for light backgrounds.
- `streammydrone-wordmark-dark.svg`: horizontal wordmark for dark backgrounds.
- `streammydrone-wordmark-light.svg`: horizontal wordmark for light backgrounds.
- `legacy/`: previous DJI Live Bridge icon sources preserved before replacement.
- `concepts/streammydrone-selected-reference.png`: selected visual exploration; the SVG is the production source.

## Palette

- Midnight navy: `#061B44`
- Signal cyan: `#20D7F1`
- Soft white: `#F8FBFF`
- Live coral: `#FF5A52`

## Usage

Keep the mark inside Android's adaptive-icon safe zone. Do not add manufacturer or social-platform logos. Generate desktop icon sizes from the master SVG with:

```sh
npm run tauri -- icon src-tauri/icons/source.svg
```
