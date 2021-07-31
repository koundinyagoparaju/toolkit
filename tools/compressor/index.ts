import Tool from "../tool";
import {ToolManifest} from "../tool-manifest";
import compressing, {zip} from 'compressing';
import Stream = zip.Stream;
import {Buffer} from "buffer";

class ZipCompressor extends Tool {
  protected manifest: ToolManifest = new ToolManifest(
      "zip-compressor",
      "Compresses given files to a zip file",
      ["zip-compressor", "compress files", "file compression"],
      "non-ui",
      ["string", "file_array"],
      ["file"]
  );

  protected process(input: Array<any>): Array<any> {
    // let compressedStream: Stream = new compressing.zip.Stream();
    // const filesToCompress: Array<File> = input[1];
    // if (filesToCompress.length === 0) throw new Error("no files provided");
    // for (const file of filesToCompress) {
    //   file
    //   .arrayBuffer()
    //   .then(fileContent =>
    //       compressedStream
    //       .addEntry(new Buffer(fileContent), {"size": fileContent.byteLength}));
    // }
    return ['todo'];
    // return compressedStream.pipe()
  }

}
