import { greet } from "./greeter";

export default class Lazy {
  render() {
    return <div>{greet}</div>;
  }
}
