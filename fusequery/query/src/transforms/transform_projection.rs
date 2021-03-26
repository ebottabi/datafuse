// Copyright 2020-2021 The FuseQuery Authors.
//
// SPDX-License-Identifier: Apache-2.0.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use common_planners::ExpressionPlan;

use crate::common_datablocks::DataBlock;
use crate::common_datavalues::DataSchemaRef;
use crate::common_functions::IFunction;
use crate::datastreams::{ExpressionStream, SendableDataBlockStream};
use crate::error::{FuseQueryError, FuseQueryResult};
use crate::processors::{EmptyProcessor, IProcessor};

pub struct ProjectionTransform {
    funcs: Vec<Box<dyn IFunction>>,
    schema: DataSchemaRef,
    input: Arc<dyn IProcessor>,
}

impl ProjectionTransform {
    pub fn try_create(schema: DataSchemaRef, exprs: Vec<ExpressionPlan>) -> FuseQueryResult<Self> {
        let mut funcs = Vec::with_capacity(exprs.len());
        for expr in &exprs {
            let func = expr.to_function()?;
            if func.is_aggregator() {
                return Err(FuseQueryError::build_internal_error(format!(
                    "Aggregate function {} is found in ProjectionTransform, should AggregatorTransform",
                    func
                )));
            }
            funcs.push(func);
        }

        Ok(ProjectionTransform {
            funcs,
            schema,
            input: Arc::new(EmptyProcessor::create()),
        })
    }

    pub fn expression_executor(
        projected_schema: &DataSchemaRef,
        block: DataBlock,
        funcs: Vec<Box<dyn IFunction>>,
    ) -> FuseQueryResult<DataBlock> {
        let mut column_values = Vec::with_capacity(funcs.len());
        for func in funcs {
            column_values.push(func.eval(&block)?.to_array(block.num_rows())?);
        }
        Ok(DataBlock::create(projected_schema.clone(), column_values))
    }
}

#[async_trait]
impl IProcessor for ProjectionTransform {
    fn name(&self) -> &str {
        "ProjectionTransform"
    }

    fn connect_to(&mut self, input: Arc<dyn IProcessor>) -> FuseQueryResult<()> {
        self.input = input;
        Ok(())
    }

    fn inputs(&self) -> Vec<Arc<dyn IProcessor>> {
        vec![self.input.clone()]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn execute(&self) -> FuseQueryResult<SendableDataBlockStream> {
        Ok(Box::pin(ExpressionStream::try_create(
            self.input.execute().await?,
            self.schema.clone(),
            self.funcs.clone(),
            ProjectionTransform::expression_executor,
        )?))
    }
}
