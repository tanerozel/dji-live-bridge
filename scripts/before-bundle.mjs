// Tauri runs one beforeBundleCommand for every platform. The camera extension
// is macOS-only, so dispatch here instead of failing on Windows.
import { execFileSync } from "node:child_process";

if (process.platform === "darwin") {
  execFileSync("./scripts/build-camera-extension.sh", { stdio: "inherit" });
} else {
  console.log("skipping the macOS camera extension build on " + process.platform);
}
