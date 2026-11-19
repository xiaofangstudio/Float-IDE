import { builtinList } from '../src/lang/builtins';
console.log(builtinList().map(b=>b.name).join(','));