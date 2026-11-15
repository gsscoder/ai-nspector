import express from "express";
import { User } from "./models";
import { greet } from "./greeter.js";
import { settings } from "./lib";

export * from "./models";

export default function main(): void {
  greet(express, settings);
}

export async function lazy() {
  const module = await import("./lazy");
  return module;
}
