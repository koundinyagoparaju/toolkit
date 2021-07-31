import {ToolManifest} from "./tool-manifest";
import InvalidArgumentError, {InvalidArgument} from "./invalid-argument-error";
import ProcessingError from "./processing-error";

export default abstract class Tool {
    protected abstract process(input: Array<any>): Array<any>;

    protected abstract manifest: ToolManifest;

    getManifest(): ToolManifest {
        return this.manifest;
    }

    getName(): string {
        return this.manifest.name;
    }

    validateInput(input: Array<any>): void {
        if (this.manifest.input.length != input.length) {
            throw new InvalidArgumentError([{
                position: 0,
                expected: `${this.manifest.input.length} values`,
                actual: `${input.length} values`
            }]);
        }
        let invalidInputs: Array<InvalidArgument> = input.map((inputValue, index) => {
            if (typeof inputValue != this.manifest.input[index]
                && (Array.isArray(inputValue)
                    && !this.manifest.input[index].includes("_array"))) {
                return {
                    position: index,
                    expected: this.manifest.input[index],
                    actual: typeof inputValue
                }
            }
        }).filter(invalidArgument => !!invalidArgument);
        if (invalidInputs.length != 0) {
            throw new InvalidArgumentError(invalidInputs);
        }
    };

    run(input: Array<any>): Array<any> | string {
        this.validateInput(input);
        try {
            return this.process(input);
        } catch (e) {
            throw new ProcessingError(e);
        }
    }
}
