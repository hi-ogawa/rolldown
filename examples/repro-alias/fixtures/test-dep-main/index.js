export const main = "main";

// [not ok]
export { sub } from "test-dep-sub";

// [ok]
// export * as sub from "test-dep-sub";
