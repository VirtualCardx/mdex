import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// Code block theme (code panes stay dark in both UI themes).
import "highlight.js/styles/github-dark.css";
// KaTeX math styles.
import "katex/dist/katex.min.css";
// App styles last so variables and overrides win.
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
