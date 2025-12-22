import nativeBinding from './index.js';
import { projection, createProjectionProxy, ProjectionRunner } from './js/index.js';

export const SpiteDbNapi = nativeBinding.SpiteDbNapi;
export const SpiteDBNapi = SpiteDbNapi;
export const DEFAULT_TENANT = 'default';
export { projection, createProjectionProxy, ProjectionRunner };
export default nativeBinding;
