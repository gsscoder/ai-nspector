import type { User } from "./models";
import { readFileSync } from "node:fs";

export class Greeter {}

export function greet(user: User) {
  return readFileSync(user.name);
}
