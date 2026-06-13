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
  }, [view]);

  return view === "settings" ? <Settings /> : <UsageDetail />;
}
