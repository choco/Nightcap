import { hydrateRoot } from "react-dom/client";
import { App } from "./App";
import "./styles/main.css";

const root = document.getElementById("root");
if (!root) throw new Error("index.html has no #root");
hydrateRoot(root, <App />);
