import { helper } from "./util";
import React from "react";
import { thing } from "@app/thing";

export function app(): number {
  return helper() + (thing as unknown as number) + (React ? 0 : 1);
}
