const native = require('../native/index.node');
const upstreamFunctions = require('../../jsonata/src/functions');

const rustFunctions = native.load_functions();

module.exports = {
    ...upstreamFunctions,
    ...rustFunctions
};
