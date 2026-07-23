import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    sourcemap: true,
  },
  test: {
    coverage: {
      provider: "v8",
      reporter: ["text", "html", "lcov"],
      include: ["src/**/*.{svelte,ts}"],
      exclude: ["src/**/*.test.ts", "src/vite-env.d.ts"],
      thresholds: {
        branches: 28,
        functions: 29,
        lines: 21,
        statements: 20,
      },
    },
  },
});
