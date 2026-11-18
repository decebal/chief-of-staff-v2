import { initFromDevice, surface, storage, compute, draw, frame, frameLoop } from "vgpu";
import { prepareShader } from "@vgpu/wgsl/prepare";
globalThis.vgpu = { initFromDevice, surface, storage, compute, draw, frame, frameLoop, prepareShader };
