import { getCurrentWindow } from "@tauri-apps/api/window";
import { createRoot } from "react-dom/client";
import App from "./App";
import { SettingsPond } from "./components/SettingsPond";
import "./styles.css";

// Both windows share this bundle; only the label decides what they render.
const isPanel = getCurrentWindow().label === "panel";

createRoot(document.getElementById("root")!).render(
  isPanel ? <SettingsPond /> : <App />,
);
