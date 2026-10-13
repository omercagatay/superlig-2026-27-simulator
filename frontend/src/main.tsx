import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { LangProvider } from "./i18n";
import "@fontsource/big-shoulders-display/800";
import "@fontsource/big-shoulders-display/900";
import "@fontsource/newsreader/400.css";
import "@fontsource/newsreader/400-italic.css";
import "@fontsource/newsreader/600.css";
import "@fontsource/archivo-narrow/400.css";
import "@fontsource/archivo-narrow/600.css";
import "@fontsource/archivo-narrow/700.css";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <LangProvider>
      <App />
    </LangProvider>
  </React.StrictMode>
);
