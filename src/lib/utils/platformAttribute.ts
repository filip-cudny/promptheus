import { isLinux } from "./platform";

if (isLinux) document.documentElement.dataset.platform = "linux";
