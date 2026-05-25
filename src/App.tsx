import { getCurrentWindow } from "@tauri-apps/api/window";
import { Overlay } from "./components/Overlay";
import { Settings } from "./components/Settings";

const windowLabel = (() => {
  try {
    return getCurrentWindow().label;
  } catch {
    return "overlay";
  }
})();

export function App() {
  if (windowLabel === "overlay") return <Overlay />;
  if (windowLabel === "settings") return <Settings />;
  return null;
}
