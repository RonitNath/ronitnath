import solid from "vite-plugin-solid";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    solid({
      hot: false,
      typescript: {
        onlyRemoveTypeImports: true,
      },
    }),
  ],
  resolve: {
    conditions: ["development", "browser"],
  },
  test: {
    environment: "jsdom",
  },
});
