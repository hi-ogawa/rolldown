import * as depDeconflict from "./deconflict.js";
export { depDeconflict };

// export const depSplit = () => import("./split.js");

export default () => require("node:util");
