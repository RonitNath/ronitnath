import { defineConfig } from "vite";
import { solidAppConfig } from "@isoastra/web-runtime/vite";

// The base URL, output directory, hashed entry naming, and manifest are the
// contract `http_runtime::StaticAssets` reads; they live in the package so a
// change to the contract cannot land in the crates and miss an application.
export default defineConfig(solidAppConfig());
