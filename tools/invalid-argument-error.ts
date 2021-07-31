export default class InvalidArgumentError implements Error {
    constructor(invalidArguments: Array<InvalidArgument>) {
        this.message = invalidArguments.map(invalidArgument =>
            `${invalidArgument.position.toString()}|${invalidArgument.expected}|${invalidArgument.actual}`)
            .join(",");
        this.name = "InvalidArgumentError";
        this.invalidArguments = invalidArguments;
    }

    message: string;
    name: string;
    invalidArguments: Array<InvalidArgument>;
}
export type InvalidArgument = { position: number, expected: string, actual: string };
