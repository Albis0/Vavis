import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./react/App";
import "./styles.css";

// The theme is applied before the app mounts, not from an effect inside it.
//
// Components read `data-theme` as they initialise -- the reactor builds a
// whole environment map from it -- and a child's effect runs before the
// parent's, so setting it there means every child initialises against the
// wrong theme and has to be corrected afterwards. Doing it here also removes
// the flash of the wrong background on a light-theme start.
const savedTheme = localStorage.getItem("vavis.theme");
document.documentElement.dataset.theme =
    savedTheme === "light" ? "light" : "dark";

const target = document.getElementById("app");
if (!target) {
    throw new Error("mount point #app is missing from index.html");
}

// No StrictMode double-invoke in development.
//
// StrictMode mounts every component twice to surface effects that are not
// cleanup-safe, which is a good check in general and the wrong one here: the
// reactor owns a WebGL context and the stores own pollers and Tauri event
// subscriptions. A second mount either doubles those or tears down the live
// one, and the failure looks like a rendering bug rather than a lifecycle
// one. The cleanups are tested directly instead.
const STRICT = false;

createRoot(target).render(
    STRICT ? (
        <StrictMode>
            <App />
        </StrictMode>
    ) : (
        <App />
    ),
);
