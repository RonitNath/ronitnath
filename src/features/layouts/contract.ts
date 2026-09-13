import { createWorkspaceLayout, type LayoutDocument, type RegistryContract } from '@isoastra/ui-layout/core';

export const OPERATOR_LAYOUT_KEY = 'ui.layout.operator';
export const operatorLayoutContract: RegistryContract = {
  copy: {
    description:'Operator heading and supporting copy',
    liveProps:['title','body'],
    fields:{
      title:{type:'string',description:'Visible heading',required:true},
      body:{type:'string',description:'Supporting copy'},
    },
    validate:(props) => typeof props.title === 'string' && (props.body === undefined || typeof props.body === 'string'),
  },
};

export function defaultOperatorLayout(): LayoutDocument {
  return {
    ...createWorkspaceLayout('configuration','operator-layout',0),
    nodes:[{id:'introduction',component:'copy',placement:{region:'primary'},props:{title:'Operator workspace',body:'A carefully staged surface for Isoastra operations.'}}],
  };
}
