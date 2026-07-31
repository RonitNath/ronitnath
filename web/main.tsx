/* @refresh reload */
import { startIslands } from "@isoastra/web-runtime/islands";
import { islandLoaders } from "./island-registry";

const dispose = startIslands(islandLoaders);
if (import.meta.hot) import.meta.hot.dispose(dispose);
