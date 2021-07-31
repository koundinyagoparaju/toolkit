export class ToolManifest {
    name: string;
    description: string;
    keywords: Array<string>;
    kind: Kind;
    input: Array<IO>;
    output: Array<IO>;

    constructor(name: string, description: string, keywords: Array<string>, kind: Kind, input: Array<IO>, output: Array<IO>) {
        this.name = name;
        this.description = description;
        this.keywords = keywords;
        this.kind = kind;
        this.input = input;
        this.output = output;
    }
}
export type IO = "string"|"number"|"file"|"string_array"|"number_array"|"file_array";
export type Kind = "ui" | "non-ui";

