import { getCurrentWindow } from "@tauri-apps/api/window";
import { createRoot } from "react-dom/client";
import App from "./App";
import { ControlPanel } from "./components/ControlPanel";
import "./styles.css";

// Both windows share this bundle; only the label decides what they render.
const isPanel = getCurrentWindow().label === "panel";

createRoot(document.getElementById("root")!).render(
  isPanel ? <ControlPanel /> : <App />,
);
