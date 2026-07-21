import { app } from "./app";

export function base(): number {
  return app ? 1 : 0;
}
