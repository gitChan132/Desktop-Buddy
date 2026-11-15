import { getCurrentWindow } from "@tauri-apps/api/window";
import { createRoot } from "react-dom/client";
import App from "./App";
import { ControlPanel } from "./components/ControlPanel";
import { SettingsPond } from "./components/SettingsPond";
import { TriggerRemote } from "./components/TriggerRemote";
import "./styles.css";

// All windows share this bundle; the label decides what each one renders.
const label = getCurrentWindow().label;

createRoot(document.getElementById("root")!).render(
  label === "panel" ? (
    <ControlPanel />
  ) : label === "pond" ? (
    <SettingsPond />
  ) : label === "remote" ? (
    <TriggerRemote />
  ) : (
    <App />
  ),
);
