import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
export default defineConfig(({ command }) => ({
  plugins: [react()],
  worker: { format: "es" },
  base: command === "build" ? "/mojo/" : "/",
}));
