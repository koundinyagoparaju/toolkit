export default class ProcessingError implements Error {
  constructor(error: Error) {
    this.message = error.message;
    this.name = "ProcessingError";
    this.stack = error.stack;
  }
  message: string;
  name: string;
  stack: string
}
