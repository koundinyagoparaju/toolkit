import {GetStaticProps, GetStaticPaths} from "next";
import fs from "fs";
import Tool from "../../tools/tool";
import {ToolManifest} from "../../tools/tool-manifest";
import {Text, Button} from '@chakra-ui/react';
import {useEffect, useState} from "react";

//Read up https://stackoverflow.com/questions/13488501/nodejs-readdir-and-require-relative-paths to know why ${process.cwd()}/tools is used at one place and ../../tools/ is used at another
export const getStaticProps: GetStaticProps = async context => {
    return {
        props: {
            toolName: context.params["toolName"]
        }
    };
}

function getToolsPath() {
    return `${process.cwd()}/tools`;
}

function getTool(toolName): Promise<Tool> {
    return import(`../../tools/${toolName}/index`).then((toolImport: any) => {
        const tool: Tool = toolImport.default;
        return tool;
    });
}

export const getStaticPaths: GetStaticPaths = async () => {
    return Promise.all(
        fs.readdirSync(getToolsPath())
            .filter(directoryItem => fs.lstatSync(`${getToolsPath()}/${directoryItem}`).isDirectory())
            .map(directoryItem => {
                return getTool(directoryItem).then(tool => {
                    return {
                        params: {
                            toolName: tool.getName()
                        }
                    }
                });
            })).then(toolNames => {
        return {
            paths: toolNames,
            fallback: false
        }
    });
}

export default function ToolPage({toolName}) {
    const [tool, setTool] = useState<Tool>(null);
    const fetchAndSetTool = async (toolName) => {
        const fetchedTool: Tool = await getTool(toolName);
        setTool(fetchedTool);
    }
    const [inputs, setInputs] = useState<Array<any>>([]);
    useEffect(() => {
        if(!tool) {
            fetchAndSetTool(toolName);
        }
    });
    if(!tool) return null;
    const manifest: ToolManifest = tool.getManifest();
    const inputFields = manifest.input.map((input, index) => {
        switch (input) {
            case "number":
                return <Text id={`${index}`} key={`${index}`} inputMode="numeric" onChange={onChange} inputProps={{'data-datatype': input}}/>;
            case "string":
                return <Text id={`${index}`} key={`${index}`} inputMode="text" onChange={onChange} inputProps={{'data-datatype': input}}/>;
            case "string_array":
            case "number_array":
                return <div id={`${index}`} key={`${index}`} onChange={onChange} data-datatype={input} >
                    <Text id={`${index}-0`} key={`${index}-0`} inputMode="text" inputProps={{'data-datatype': input}}/>
                    <Text id={`${index}-1`} key={`${index}-1`} inputMode="text" inputProps={{'data-datatype': 'delimiter'}}/>
                </div>;
            case "file":
            case "file_array":
                return null
            // case "number_array": return <Text id={`input-${index}`} inputMode="numeric"/>;
        }
    });
    const outputFields = manifest.output.map((output, index) => {
        switch (output) {
            case "file":
            case "file_array":



        }
    });
    if (manifest.kind === "non-ui") {
        return (
            <div>
                <h1>{manifest.name}</h1>
                <h3>{manifest.description}</h3>
                <div id="input">
                    {inputFields}
                    <Button name="Run tool" onClick={onClick}>Run tool</Button>
                </div>
                <div id="output">

                </div>
            </div>);

    }

    function onClick(): void {
        const result = tool.run(inputs);
        if(typeof result === 'string') {
            console.log(result);
        } else {
            console.log(result[0])
        }
    }
    function onChange(e): void {
        const id = e.target.id;
        const type: string = e.target.dataset.datatype;
        let index: number = id.includes("-") ? parseInt(id.substring(0, id.indexOf('-'))) : parseInt(id);
        if(["string", "number", "file", "file_array"].includes(type)) {
            switch (type) {
                case "string":
                    inputs[index] = e.target.value;
                    break;
                case "number":
                    inputs[index] = parseInt(e.target.value);
                    break;
                case "file":
                    inputs[index] = e.target.files[0];
                    break;
                case "file_array":
                    inputs[index] = e.target.files;
            }
        } else {
            let data:string = '', delimiter: string = '', datatype:string = '';
            if(type === 'delimiter') {
                delimiter = e.target.value;
                //used any as type because return type of document.getElementById doesn't allow accessing the member value
                let dataElement: any = document.getElementById(`${index}-0`);
                data = dataElement.value;
                datatype = dataElement.dataset.datatype;
            } else {
                data = e.target.value;
                let delimiterElement: any = document.getElementById(`${index}-1`);
                delimiter = delimiterElement.value;
                datatype = e.target.dataset.datatype;
            }
            inputs[index] = data.split(delimiter).map(value => {
                if(datatype == "number_array") {
                    return parseInt(value);
                } else {
                    return value;
                }
            });
        }
        setInputs(inputs);
        console.log(inputs);
    }
}

