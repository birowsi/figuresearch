declare module "encoding-japanese" {
  type ConvertOptions = {
    to: string;
    from: string;
    type: "ARRAY";
  };

  const Encoding: {
    convert(input: string, options: ConvertOptions): number[];
  };

  export default Encoding;
}
