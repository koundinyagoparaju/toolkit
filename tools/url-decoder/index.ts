import Tool from "../tool";
import {ToolManifest} from "../tool-manifest";

class UrlDecoder extends Tool {
  protected manifest: ToolManifest = new ToolManifest("url-decoder",
      "Decode a url-encoded string",
      ["url decoder"],
      "non-ui",
      ["string"],
      ["string"]);

  protected process(input: Array<any>): Array<any> {
    return [decodeURIComponent(input[0])];
  }
}

export default new UrlDecoder();
