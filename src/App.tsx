import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import VideoTrimmer from "./VideoTrimmer";
import "./styles.css";
export default function App() {
  // The window starts hidden; show it after the first commit (rAF stalls while hidden).
  useEffect(() => {
    void getCurrentWindow()
      .show()
      .catch(() => {});
  }, []);
  return <VideoTrimmer />;
}
