// Tauri runs one beforeBundleCommand for every platform, and each platform has
// a native camera to build first: a system extension on macOS, a DirectShow
// filter on Windows. Dispatch here instead of failing on the other platform.
import { execFileSync } from "node:child_process";

if (process.platform === "darwin") {
  execFileSync("./scripts/build-camera-extension.sh", { stdio: "inherit" });
} else if (process.platform === "win32") {
  execFileSync("powershell", ["-NoProfile", "-File", "scripts/build-windows-camera.ps1"], {
    stdio: "inherit",
  });
} else {
  console.log("skipping the camera build on " + process.platform);
}
