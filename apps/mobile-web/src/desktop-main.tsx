import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { DesktopWorkbench } from "./desktop/DesktopWorkbench";
import "./index.css";

const root = document.getElementById("app");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <DesktopWorkbench />
    </StrictMode>,
  );
}
