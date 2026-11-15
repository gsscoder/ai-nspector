export interface User {
  name: string;
}

export enum Role {
  Admin,
}

export type Id = string;

export abstract class Base {}

const hidden = () => 1;
