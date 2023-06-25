import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vitejs.dev/config/
export default defineConfig({
  root: 'app',
  publicDir: 'static',
  server: {
    port: 5801,
    strictPort : true,
  },
  plugins: [react()],
});
