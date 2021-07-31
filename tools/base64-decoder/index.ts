import Tool from "../tool";
import {ToolManifest} from "../tool-manifest";

class Base64Decoder extends Tool {
  process(input: Array<any>): Array<any> {
    return [atob(input[0])];
  }

  manifest: ToolManifest = new ToolManifest(
      "base64-decoder",
      "Decodes base64 string",
      ["base64-decoder", "base64 decoder"],
      "non-ui",
      ["string"],
      ["string"]
  );
}

export default new Base64Decoder();
