import { useEffect, useState } from "react";
import Settings from "./components/Settings";
import UsageDetail from "./components/UsageDetail";

type View = "settings" | "panel";

function currentView(): View {
  const params = new URLSearchParams(window.location.search);
  return params.get("view") === "settings" ? "settings" : "panel";
}

export default function App() {
  const [view] = useState<View>(currentView);

  useEffect(() => {
    document.documentElement.dataset.view = view;
    // Only macOS gets a real NSVisualEffectView behind the panel; everywhere
    // else the panel must paint a solid fill instead of relying on vibrancy.
    const isMac = navigator.userAgent.includes("Mac");
    document.documentElement.classList.toggle("no-vibrancy", !isMac);
  }, [view]);

  return view === "settings" ? <Settings /> : <UsageDetail />;
}
