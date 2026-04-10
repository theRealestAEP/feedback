import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";

import { MainShell } from "./components/MainShell";
import { SettingsShell } from "./components/SettingsShell";
import "./App.css";

function App() {
  const isSettingsWindow = getCurrentWindow().label === "settings";

  useEffect(() => {
    document.documentElement.classList.toggle("settings-window", isSettingsWindow);
    document.body.classList.toggle("settings-window", isSettingsWindow);

    return () => {
      document.documentElement.classList.remove("settings-window");
      document.body.classList.remove("settings-window");
    };
  }, [isSettingsWindow]);

  return isSettingsWindow ? <SettingsShell /> : <MainShell />;
}

export default App;
