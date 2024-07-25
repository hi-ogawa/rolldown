export const main = "main";

// [not ok]
import { sub } from "test-dep-sub";

// [ok]
// import * as sub from "test-dep-sub";
export { sub }
